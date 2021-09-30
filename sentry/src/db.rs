use deadpool_postgres::{Manager, ManagerConfig, RecyclingMethod};
use redis::aio::MultiplexedConnection;
use std::env;
use tokio_postgres::NoTls;

use lazy_static::lazy_static;

pub mod accounting;
pub mod analytics;
pub mod campaign;
mod channel;
pub mod event_aggregate;
pub mod spendable;
mod validator_message;

pub use self::campaign::*;
pub use self::channel::*;
pub use self::event_aggregate::*;
pub use self::validator_message::*;

// Re-export the Postgres PoolError for easier usages
pub use deadpool_postgres::PoolError;
// Re-export the redis RedisError for easier usage
pub use redis::RedisError;

pub type DbPool = deadpool_postgres::Pool;

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
    static ref POSTGRES_CONFIG: tokio_postgres::Config = {
        let mut config = tokio_postgres::Config::new();

        config
            .user(POSTGRES_USER.as_str())
            .password(POSTGRES_PASSWORD.as_str())
            .host(POSTGRES_HOST.as_str())
            .port(*POSTGRES_PORT);
        if let Some(db) = POSTGRES_DB.as_ref() {
            config.dbname(db);
        }

        config
    };
}

pub async fn redis_connection(url: &str) -> Result<MultiplexedConnection, RedisError> {
    let client = redis::Client::open(url)?;

    client.get_multiplexed_async_connection().await
}

pub async fn postgres_connection(max_size: usize) -> DbPool {
    let mgr_config = ManagerConfig {
        recycling_method: RecyclingMethod::Verified,
    };

    let manager = Manager::from_config(POSTGRES_CONFIG.clone(), NoTls, mgr_config);

    DbPool::new(manager, max_size)
}

pub async fn setup_migrations(environment: &str) {
    use migrant_lib::{Config, Direction, Migrator, Settings};

    let settings = Settings::configure_postgres()
        .database_user(POSTGRES_USER.as_str())
        .database_password(POSTGRES_PASSWORD.as_str())
        .database_host(POSTGRES_HOST.as_str())
        .database_port(*POSTGRES_PORT)
        .database_name(POSTGRES_DB.as_ref().unwrap_or(&POSTGRES_USER))
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
    // `tests_postgres::MIGRATIONS`
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
pub mod tests_postgres {
    use std::{
        ops::{Deref, DerefMut},
        sync::atomic::{AtomicUsize, Ordering},
    };

    use deadpool::managed::{Manager as ManagerTrait, RecycleResult};
    use deadpool_postgres::ManagerConfig;
    use once_cell::sync::Lazy;
    use tokio_postgres::{NoTls, SimpleQueryMessage};

    use async_trait::async_trait;

    use super::{DbPool, PoolError, POSTGRES_CONFIG};

    pub type Pool = deadpool::managed::Pool<Manager>;

    pub static DATABASE_POOL: Lazy<Pool> = Lazy::new(|| create_pool("test"));

    /// we must have a duplication of the migration because of how migrant is handling migrations
    /// we need to separately setup test migrations
    pub static MIGRATIONS: &[&str] = &["20190806011140_initial-tables"];

    fn create_pool(db_prefix: &str) -> Pool {
        let manager_config = ManagerConfig {
            // to guarantee that `is_closed()` & test query will be ran to determine bad connections
            recycling_method: deadpool_postgres::RecyclingMethod::Verified,
        };
        let manager = Manager::new(POSTGRES_CONFIG.clone(), manager_config, db_prefix);

        Pool::new(manager, 15)
    }

    /// A Database is used to isolate test runs from each other
    /// we need to know the name of the database we've created.
    /// This will allow us the drop the database when we are recycling the connection
    pub struct Database {
        /// The database name that will be created by the pool `CREATE DATABASE`
        /// This database will be set on configuration level of the underlying connection Pool for tests
        pub name: String,
        pub pool: deadpool_postgres::Pool,
    }

    impl Database {
        pub fn new(name: String, pool: DbPool) -> Self {
            Self { name, pool }
        }
    }

    impl Deref for Database {
        type Target = deadpool_postgres::Pool;

        fn deref(&self) -> &deadpool_postgres::Pool {
            &self.pool
        }
    }

    impl DerefMut for Database {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.pool
        }
    }

    impl AsRef<deadpool_postgres::Pool> for Database {
        fn as_ref(&self) -> &deadpool_postgres::Pool {
            &self.pool
        }
    }

    impl AsMut<deadpool_postgres::Pool> for Database {
        fn as_mut(&mut self) -> &mut deadpool_postgres::Pool {
            &mut self.pool
        }
    }

    /// Base Pool and Config are used to create a new DATABASE and later on
    /// create the actual connection to the database with default options set
    pub struct Manager {
        base_config: tokio_postgres::Config,
        base_pool: deadpool_postgres::Pool,
        manager_config: ManagerConfig,
        index: AtomicUsize,
        db_prefix: String,
    }

    impl Manager {
        pub fn new(
            base_config: tokio_postgres::Config,
            manager_config: ManagerConfig,
            db_prefix: &str,
        ) -> Self {
            // We need to create the schema with a temporary connection, in order to use it for the real Test Pool
            let base_manager = deadpool_postgres::Manager::from_config(
                base_config.clone(),
                NoTls,
                manager_config.clone(),
            );
            let base_pool = deadpool_postgres::Pool::new(base_manager, 15);

            Self::new_with_pool(base_pool, base_config, manager_config, db_prefix)
        }

        pub fn new_with_pool(
            base_pool: deadpool_postgres::Pool,
            base_config: tokio_postgres::Config,
            manager_config: ManagerConfig,
            db_prefix: &str,
        ) -> Self {
            Self {
                base_config,
                base_pool,
                manager_config,
                index: AtomicUsize::new(0),
                db_prefix: db_prefix.into(),
            }
        }
    }

    #[async_trait]
    impl ManagerTrait for Manager {
        type Type = Database;

        type Error = PoolError;

        async fn create(&self) -> Result<Self::Type, Self::Error> {
            let pool_index = self.index.fetch_add(1, Ordering::SeqCst);

            // e.g. test_0, test_1, test_2
            let db_name = format!("{}_{}", self.db_prefix, pool_index);

            // 1. Drop the database if it exists - if a test failed before, the database wouldn't have been removed
            // 2. Create database
            let drop_db = format!("DROP DATABASE IF EXISTS {0} WITH (FORCE);", db_name);
            let created_db = format!("CREATE DATABASE {0};", db_name);
            let temp_client = self.base_pool.get().await?;

            let drop_db_result = temp_client.simple_query(drop_db.as_str()).await?;
            assert_eq!(1, drop_db_result.len());
            assert!(matches!(
                drop_db_result[0],
                SimpleQueryMessage::CommandComplete(..)
            ));

            let create_db_result = temp_client.simple_query(created_db.as_str()).await?;
            assert_eq!(1, create_db_result.len());
            assert!(matches!(
                create_db_result[0],
                SimpleQueryMessage::CommandComplete(..)
            ));

            let mut config = self.base_config.clone();
            // set the database in the configuration of the inside Pool (used for tests)
            config.dbname(&db_name);

            let manager =
                deadpool_postgres::Manager::from_config(config, NoTls, self.manager_config.clone());
            let pool = deadpool_postgres::Pool::new(manager, 15);

            Ok(Database::new(db_name, pool))
        }

        async fn recycle(&self, database: &mut Database) -> RecycleResult<Self::Error> {
            // DROP the public schema and create it again for usage after recycling
            let queries = format!("DROP SCHEMA public CASCADE; CREATE SCHEMA public;");

            if database.pool.is_closed() {
                let mut config = self.base_config.clone();
                // set the database in the configuration of the inside Pool (used for tests)
                config.dbname(&database.name);

                let manager = deadpool_postgres::Manager::from_config(
                    config,
                    NoTls,
                    self.manager_config.clone(),
                );
                let pool = deadpool_postgres::Pool::new(manager, 15);

                database.pool = pool;
            }

            let result = database
                .pool
                .get()
                .await?
                .simple_query(&queries)
                .await
                .map_err(|err| PoolError::Backend(err))
                .expect("Should not error");
            assert_eq!(2, result.len());
            assert!(matches!(result[0], SimpleQueryMessage::CommandComplete(..)));
            assert!(matches!(result[1], SimpleQueryMessage::CommandComplete(..)));

            Ok(())
        }
    }

    pub async fn setup_test_migrations(pool: DbPool) -> Result<(), PoolError> {
        let client = pool.get().await?;

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

        Ok(client.batch_execute(&full_query).await?)
    }

    #[cfg(test)]
    mod test {
        use super::*;

        #[tokio::test]
        /// Does not use the `DATABASE_POOL` as other tests can interfere with the pool objects!
        async fn test_postgres_pool() {
            let pool = create_pool("testing_pool");

            let database_1 = pool.get().await.expect("Should get");
            let status = pool.status();
            assert_eq!(status.size, 1);
            assert_eq!(status.available, 0);

            let database_2 = pool.get().await.expect("Should get");
            let status = pool.status();
            assert_eq!(status.size, 2);
            assert_eq!(status.available, 0);

            drop(database_1);
            let status = pool.status();
            assert_eq!(status.size, 2);
            assert_eq!(status.available, 1);

            drop(database_2);
            let status = pool.status();
            assert_eq!(status.size, 2);
            assert_eq!(status.available, 2);

            let database_3 = pool.get().await.expect("Should get");
            let status = pool.status();
            assert_eq!(status.size, 2);
            assert_eq!(status.available, 1);

            let database_4 = pool.get().await.expect("Should get");
            let status = pool.status();
            assert_eq!(status.size, 2);
            assert_eq!(status.available, 0);

            let database_5 = pool.get().await.expect("Should get");
            let status = pool.status();
            assert_eq!(status.size, 3);
            assert_eq!(status.available, 0);

            drop(database_3);
            drop(database_4);
            drop(database_5);
            let status = pool.status();
            assert_eq!(status.size, 3);
            assert_eq!(status.available, 3);
        }
    }
}

#[cfg(test)]
pub mod redis_pool {

    use dashmap::DashMap;
    use deadpool::managed::{Manager as ManagerTrait, RecycleResult};
    use thiserror::Error;

    use crate::db::redis_connection;
    use async_trait::async_trait;

    use once_cell::sync::Lazy;

    use super::*;

    pub type Pool = deadpool::managed::Pool<Manager>;

    pub static TESTS_POOL: Lazy<Pool> =
        Lazy::new(|| Pool::new(Manager::new(), Manager::CONNECTIONS.into()));

    #[derive(Clone)]
    pub struct Database {
        available: bool,
        index: u8,
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
        #[error("A redis error occurred")]
        Redis(#[from] RedisError),
        #[error("Creation of new database connection failed")]
        CreationFailed,
    }

    #[async_trait]
    impl ManagerTrait for Manager {
        type Type = Database;
        type Error = Error;

        async fn create(&self) -> Result<Self::Type, Self::Error> {
            for mut record in self.connections.iter_mut() {
                let database = record.value_mut().as_mut();

                match database {
                    Some(database) if database.available => {
                        database.available = false;
                        return Ok(database.clone());
                    }
                    // if Some but not available, skip it
                    Some(database) if !database.available => continue,
                    // if there is no connection or it's available
                    // always create a new redis connection because of a known issue in redis
                    // see https://github.com/mitsuhiko/redis-rs/issues/325
                    _ => {
                        let mut redis_conn =
                            redis_connection(&format!("{}{}", Self::URL, record.key()))
                                .await
                                .expect("Should connect");

                        // run `FLUSHDB` to clean any leftovers of previous tests
                        // even from different test runs as there might be leftovers
                        // flush never fails as an operation
                        Self::flush_db(&mut redis_conn).await.expect("Should flush");

                        let database = Database {
                            available: false,
                            index: *record.key(),
                            connection: redis_conn,
                        };

                        *record.value_mut() = Some(database.clone());

                        return Ok(database);
                    }
                }
            }

            Err(Error::CreationFailed)
        }

        async fn recycle(&self, database: &mut Database) -> RecycleResult<Self::Error> {
            // always make a new connection because of know redis crate issue
            // see https://github.com/mitsuhiko/redis-rs/issues/325
            let connection = redis_connection(&format!("{}{}", Self::URL, database.index))
                .await
                .expect("Should connect");
            // make the database available
            database.available = true;
            database.connection = connection;
            Self::flush_db(&mut database.connection)
                .await
                .expect("Should flush");

            Ok(())
        }
    }
}
