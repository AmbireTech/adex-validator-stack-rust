use bb8::Pool;
use bb8_postgres::{tokio_postgres::NoTls, PostgresConnectionManager};
use redis::{aio::MultiplexedConnection, RedisError};
use std::env;

use lazy_static::lazy_static;

pub mod analytics;
mod channel;
pub mod event_aggregate;
pub mod spendable;
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
    if let Some(db) = POSTGRES_DB.as_ref() {
        config.dbname(db);
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

    // NOTE: Make sure to update list of migrations for the tests as well!
    // `postgres_pool::MIGRATIONS`
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
pub mod postgres_pool {
    use std::{
        ops::{Deref, DerefMut},
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        },
    };

    use deadpool::managed::{Manager as ManagerTrait, RecycleResult};
    use deadpool_postgres::ClientWrapper;
    use once_cell::sync::Lazy;
    use tokio_postgres::{
        tls::{MakeTlsConnect, TlsConnect},
        Client, Error, SimpleQueryMessage, Socket,
    };

    use async_trait::async_trait;

    use super::{POSTGRES_DB, POSTGRES_HOST, POSTGRES_PASSWORD, POSTGRES_PORT, POSTGRES_USER};

    pub type Pool = deadpool::managed::Pool<Schema, Error>;

    /// we must have a duplication of the migration because of how migrant is handling migratoins
    /// we need to separately setup test migrations
    pub static MIGRATIONS: &[&str] = &["20190806011140_initial-tables"];

    pub static TESTS_POOL: Lazy<Pool> = Lazy::new(|| {
        use deadpool_postgres::{ManagerConfig, RecyclingMethod};
        use tokio_postgres::tls::NoTls;
        let mut config = bb8_postgres::tokio_postgres::Config::new();

        config
            .user(POSTGRES_USER.as_str())
            .password(POSTGRES_PASSWORD.as_str())
            .host(POSTGRES_HOST.as_str())
            .port(*POSTGRES_PORT);
        if let Some(db) = POSTGRES_DB.as_ref() {
            config.dbname(db);
        }

        let deadpool_manager = deadpool_postgres::Manager::from_config(
            config,
            NoTls,
            ManagerConfig {
                recycling_method: RecyclingMethod::Verified,
            },
        );

        Pool::new(
            Manager {
                postgres_manager: Arc::new(deadpool_manager),
                index: AtomicUsize::new(0),
            },
            15,
        )
    });

    /// A Scheme is used to isolate test runs from each other
    /// we need to know the name of the schema we've created.
    /// This will allow us the drop the schema when we are recycling the connection
    pub struct Schema {
        /// The schema name that will be created by the pool `CREATE SCHEMA`
        /// This schema will be set as the connection `search_path` (`SET SCHEMA` for short)
        pub name: String,
        pub client: ClientWrapper,
    }

    impl Deref for Schema {
        type Target = tokio_postgres::Client;
        fn deref(&self) -> &tokio_postgres::Client {
            &self.client
        }
    }

    impl DerefMut for Schema {
        fn deref_mut(&mut self) -> &mut tokio_postgres::Client {
            &mut self.client
        }
    }

    struct Manager<T: MakeTlsConnect<Socket> + Send + Sync> {
        postgres_manager: Arc<deadpool_postgres::Manager<T>>,
        index: AtomicUsize,
    }

    #[async_trait]
    impl<T> ManagerTrait<Schema, tokio_postgres::Error> for Manager<T>
    where
        T: MakeTlsConnect<Socket> + Clone + Sync + Send + 'static,
        T::Stream: Sync + Send,
        T::TlsConnect: Sync + Send,
        <T::TlsConnect as TlsConnect<Socket>>::Future: Send,
    {
        async fn create(&self) -> Result<Schema, tokio_postgres::Error> {
            let client = self.postgres_manager.create().await?;

            let conn_index = self.index.fetch_add(1, Ordering::SeqCst);
            let schema_name = format!("test_{}", conn_index);

            // 1. Drop the schema if it exists - if a test failed before, the schema wouldn't have been removed
            // 2. Create schema
            // 3. Set the `search_path` (SET SCHEMA) - this way we don't have to define schema on queries or table creation

            let queries = format!(
                "DROP SCHEMA IF EXISTS {0} CASCADE; CREATE SCHEMA {0}; SET SESSION SCHEMA '{0}';",
                schema_name
            );

            let result = client.simple_query(&queries).await?;

            assert_eq!(3, result.len());
            assert!(matches!(result[0], SimpleQueryMessage::CommandComplete(..)));
            assert!(matches!(result[1], SimpleQueryMessage::CommandComplete(..)));
            assert!(matches!(result[2], SimpleQueryMessage::CommandComplete(..)));

            Ok(Schema {
                name: schema_name,
                client,
            })
        }

        async fn recycle(&self, schema: &mut Schema) -> RecycleResult<tokio_postgres::Error> {
            let queries = format!("DROP SCHEMA {0} CASCADE;", schema.name);
            let result = schema.simple_query(&queries).await?;
            assert_eq!(2, result.len());
            assert!(matches!(result[0], SimpleQueryMessage::CommandComplete(..)));
            assert!(matches!(result[1], SimpleQueryMessage::CommandComplete(..)));

            self.postgres_manager.recycle(&mut schema.client).await
        }
    }

    pub async fn setup_test_migrations(client: &Client) -> Result<(), Error> {
        let full_query: String = MIGRATIONS
            .iter()
            .map(|migration| {
                use std::{
                    fs::File,
                    io::{BufReader, Read},
                };
                let file = File::open(format!("migrations/{}/up.sql", migration))
                    .expect("File migration couldn't be opened");
                let mut buf_reader = BufReader::new(file);
                let mut contents = String::new();

                buf_reader
                    .read_to_string(&mut contents)
                    .expect("File migration couldn't be read");
                contents
            })
            .collect();

        client.batch_execute(&full_query).await
    }
}

#[cfg(test)]
pub mod redis_pool {

    use dashmap::DashMap;
    use deadpool::managed::{Manager as ManagerTrait, RecycleError, RecycleResult};
    use thiserror::Error;

    use crate::db::redis_connection;
    use async_trait::async_trait;

    use once_cell::sync::Lazy;

    use super::*;

    pub type Pool = deadpool::managed::Pool<Database, Error>;

    pub static TESTS_POOL: Lazy<Pool> =
        Lazy::new(|| Pool::new(Manager::new(), Manager::CONNECTIONS.into()));

    #[derive(Clone)]
    pub struct Database {
        available: bool,
        pub connection: MultiplexedConnection,
    }

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

    impl Default for Manager {
        fn default() -> Self {
            Self::new()
        }
    }

    impl Manager {
        /// The maximum databases that Redis has by default is 16, with DB `0` as default.
        const CONNECTIONS: u8 = 16;
        /// The default URL for connecting to the different databases
        const URL: &'static str = "redis://127.0.0.1:6379/";

        pub fn new() -> Self {
            Self {
                connections: (0..Self::CONNECTIONS)
                    .into_iter()
                    .map(|database_index| (database_index, None))
                    .collect(),
            }
        }

        /// Flushing (`FLUSDB`) is synchronous by default in Redis
        pub async fn flush_db(connection: &mut MultiplexedConnection) -> Result<String, Error> {
            redis::cmd("FLUSHDB")
                .query_async::<_, String>(connection)
                .await
                .map_err(Error::Redis)
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
                        database.available = false;
                        return Ok(database.clone());
                    }
                    // if Some but not available, skip it
                    Some(_) => continue,
                    None => {
                        let mut redis_conn =
                            redis_connection(&format!("{}{}", Self::URL, record.key())).await?;

                        // run `FLUSHDB` to clean any leftovers of previous tests
                        // even from different test runs as there might be leftovers
                        Self::flush_db(&mut redis_conn).await?;

                        let database = Database {
                            available: false,
                            connection: redis_conn,
                        };

                        *record.value_mut() = Some(database.clone());

                        return Ok(database);
                    }
                }
            }

            Err(Error::OutOfBound)
        }

        async fn recycle(&self, database: &mut Database) -> RecycleResult<Error> {
            // run `FLUSHDB` to clean any leftovers of previous tests
            Self::flush_db(&mut database.connection)
                .await
                .map_err(RecycleError::Backend)?;
            database.available = true;

            Ok(())
        }
    }
}
