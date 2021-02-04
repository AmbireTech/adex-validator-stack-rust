use bb8::Pool;
use bb8_postgres::{tokio_postgres::NoTls, PostgresConnectionManager};
use redis::{aio::MultiplexedConnection, RedisError};
use std::env;

use lazy_static::lazy_static;

pub mod analytics;
mod channel;
pub mod event_aggregate;
mod validator_message;

pub use self::channel::*;
pub use self::event_aggregate::*;
pub use self::validator_message::*;

pub type DbPool = Pool<PostgresConnectionManager<NoTls>>;

lazy_static! {
    static ref POSTGRES_USER: String =
        env::var("POSTGRES_USER").unwrap_or_else(|_| String::from("postgres"));
    static ref POSTGRES_PASSWORD: String =
        env::var("POSTGRES_PASSWORD").unwrap_or_else(|_| String::from("postgres"));
    static ref POSTGRES_HOST: String =
        env::var("POSTGRES_HOST").unwrap_or_else(|_| String::from("localhost"));
    static ref POSTGRES_PORT: u16 = env::var("POSTGRES_PORT")
        .unwrap_or_else(|_| String::from("5432"))
        .parse()
        .unwrap();
    static ref POSTGRES_DB: Option<String> = env::var("POSTGRES_DB").ok();
}

pub async fn redis_connection(url: &str) -> Result<MultiplexedConnection, RedisError> {
    let client = redis::Client::open(url)?;

    client.get_multiplexed_async_connection().await
}

pub async fn postgres_connection() -> Result<DbPool, bb8_postgres::tokio_postgres::Error> {
    let mut config = bb8_postgres::tokio_postgres::Config::new();

    config
        .user(POSTGRES_USER.as_str())
        .password(POSTGRES_PASSWORD.as_str())
        .host(POSTGRES_HOST.as_str())
        .port(*POSTGRES_PORT);
    if let Some(db) = POSTGRES_DB.clone() {
        config.dbname(&db);
    }
    let pg_mgr = PostgresConnectionManager::new(config, NoTls);

    Pool::builder().build(pg_mgr).await
}

pub async fn setup_migrations(environment: &str) {
    use migrant_lib::{Config, Direction, Migrator, Settings};

    let settings = Settings::configure_postgres()
        .database_user(POSTGRES_USER.as_str())
        .database_password(POSTGRES_PASSWORD.as_str())
        .database_host(POSTGRES_HOST.as_str())
        .database_port(*POSTGRES_PORT)
        .database_name(&POSTGRES_DB.as_ref().unwrap_or(&POSTGRES_USER))
        .build()
        .expect("Should build migration settings");

    let mut config = Config::with_settings(&settings);
    config.setup().expect("Should setup Postgres connection");
    // Toggle setting so tags are validated in a cli compatible manner.
    // This needs to happen before any call to `Config::use_migrations` or `Config::reload`
    config.use_cli_compatible_tags(true);

    macro_rules! make_migration {
        ($tag:expr) => {
            migrant_lib::EmbeddedMigration::with_tag($tag)
                .up(include_str!(concat!("../migrations/", $tag, "/up.sql")))
                .down(include_str!(concat!("../migrations/", $tag, "/down.sql")))
                .boxed()
        };
    }

    let mut migrations = vec![make_migration!("20190806011140_initial-tables")];

    if environment == "development" {
        // seeds database tables for testing
        migrations.push(make_migration!("20190806011140_initial-tables/seed"));
    }

    // Define Migrations
    config
        .use_migrations(&migrations)
        .expect("Loading migrations failed");

    // Reload config, ping the database for applied migrations
    let config = config.reload().expect("Should reload applied migrations");

    if environment == "development" {
        // delete all existing data to make tests reproducible
        Migrator::with_config(&config)
            .all(true)
            .direction(Direction::Down)
            .swallow_completion(true)
            .apply()
            .expect("Applying migrations failed");
    }

    let config = config.reload().expect("Should reload applied migrations");

    Migrator::with_config(&config)
        // set `swallow_completion` to `true`
        // so no error will be returned if all migrations have already been ran
        .swallow_completion(true)
        .show_output(true)
        .direction(Direction::Up)
        .all(true)
        .apply()
        .expect("Applying migrations failed");

    let _config = config
        .reload()
        .expect("Reloading config for migration failed");
}

#[cfg(test)]
pub mod redis_pool {

    use dashmap::DashMap;
    use deadpool::managed::{Manager as ManagerTrait, RecycleResult};
    // use redis::aio::ConnectionLike;
    use thiserror::Error;

    use crate::db::redis_connection;
    use async_trait::async_trait;

    use once_cell::sync::Lazy;

    use super::*;

    pub type Pool = deadpool::managed::Pool<Database, Error>;

    pub static TESTS_POOL: Lazy<Pool> = Lazy::new(|| {
        Pool::new(
            Manager::new(16),
            16,
        )
    });

    #[derive(Clone)]
    pub struct Database {
        available: bool,
        pub connection: MultiplexedConnection,
    }

    // impl ConnectionLike for Database {
    //     fn req_packed_command<'a>(&'a mut self, cmd: &'a redis::Cmd) -> redis::RedisFuture<'a, redis::Value> {
    //         self.connection.req_packed_command(cmd)
    //     }

    //     fn req_packed_commands<'a>(
    //     &'a mut self,
    //     cmd: &'a redis::Pipeline,
    //     offset: usize,
    //     count: usize,
    // ) -> redis::RedisFuture<'a, Vec<redis::Value>> {
    //     self.connection.req_packed_commands(cmd, offset, count)
    // }

    //     fn get_db(&self) -> i64 {
    //         self.connection.get_db()
    //     }
    // }

    impl std::ops::Deref for Database {
        type Target = MultiplexedConnection;

        fn deref(&self) -> &Self::Target {
            &self.connection
        }
    }

    impl std::ops::DerefMut for Database {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.connection
        }
    }


    pub struct Manager {
        connections: DashMap<u8, Option<Database>>,
    }

    impl Manager {
        pub fn new(size: u8) -> Self {
            Self {
                connections: (0..size)
                    .into_iter()
                    .map(|conn_index| (conn_index, None))
                    .collect(),
            }
        }

        pub async fn flush_db(
            connection: &mut MultiplexedConnection,
        ) -> Result<String, RedisError> {
            redis::cmd("FLUSHDB")
                .query_async::<_, String>(connection)
                .await
        }
    }

    #[derive(Debug, Error)]
    pub enum Error {
        // when we can't create more databases and all are used
        #[error("No more databases can be created")]
        OutOfBound,
        #[error("A redis error occurred")]
        Redis(#[from] RedisError),
    }

    #[async_trait]
    impl ManagerTrait<Database, Error> for Manager {
        async fn create(&self) -> Result<Database, Error> {
            for mut record in self.connections.iter_mut() {
                let database = record.value_mut().as_mut();

                match database {
                    Some(database) if database.available => {
                        // run `FLUSHDB` to clean any leftovers of previous tests
                        Self::flush_db(&mut database.connection).await?;

                        database.available = false;
                        return Ok(database.clone());
                    }
                    // if Some but not available, skip it
                    Some(_) => continue,
                    None => {
                        let redis_conn =
                            redis_connection(&format!("redis://127.0.0.1:6379/{}", record.key()))
                                .await?;

                        let database = Database {
                            available: false,
                            connection: redis_conn.clone(),
                        };

                        *record.value_mut() = Some(database.clone());

                        return Ok(database);
                    }
                }
            }

            Err(Error::OutOfBound)
        }

        async fn recycle(&self, database: &mut Database) -> RecycleResult<Error> {
            database.available = true;

            Ok(())
        }
    }
}
