use deadpool_postgres::{Manager, ManagerConfig, RecyclingMethod};
use primitives::{
    config::Environment,
    postgres::{POSTGRES_DB, POSTGRES_HOST, POSTGRES_PASSWORD, POSTGRES_PORT, POSTGRES_USER},
};
use redis::{aio::MultiplexedConnection, IntoConnectionInfo};
use std::str::FromStr;
use tokio_postgres::{
    types::{accepts, FromSql, Type},
    NoTls,
};

pub mod accounting;
pub mod analytics;
pub mod campaign;
mod channel;
pub mod spendable;
pub mod validator_message;

pub use self::campaign::*;
pub use self::channel::*;

// Re-export the Postgres Config
pub use tokio_postgres::Config as PostgresConfig;

// Re-export the Postgres PoolError for easier usages
pub use deadpool_postgres::PoolError;
// Re-export the redis RedisError for easier usage
pub use redis::RedisError;

pub type DbPool = deadpool_postgres::Pool;

pub struct TotalCount(pub u64);
impl<'a> FromSql<'a> for TotalCount {
    fn from_sql(
        ty: &Type,
        raw: &'a [u8],
    ) -> Result<Self, Box<dyn std::error::Error + Sync + Send>> {
        let str_slice = <&str as FromSql>::from_sql(ty, raw)?;

        Ok(Self(u64::from_str(str_slice)?))
    }

    // Use a varchar or text, since otherwise `int8` fails deserialization
    accepts!(VARCHAR, TEXT);
}

pub async fn redis_connection(
    url: impl IntoConnectionInfo,
) -> Result<MultiplexedConnection, RedisError> {
    let client = redis::Client::open(url)?;

    client.get_multiplexed_async_connection().await
}

/// Uses the default `max_size` of the `PoolConfig` which is `num_cpus::get_physical() * 4`
pub async fn postgres_connection(
    config: tokio_postgres::Config,
) -> Result<DbPool, deadpool_postgres::BuildError> {
    let mgr_config = ManagerConfig {
        recycling_method: RecyclingMethod::Verified,
    };

    let manager = Manager::from_config(config, NoTls, mgr_config);

    // use default max_size which is set by PoolConfig::default()
    // num_cpus::get_physical() * 4
    DbPool::builder(manager).build()
}

/// Sets the migrations using the `POSTGRES_*` environment variables
pub fn setup_migrations(environment: Environment) {
    use migrant_lib::{Config, Direction, Migrator, Settings};

    let settings = Settings::configure_postgres()
        .database_user(POSTGRES_USER.as_str())
        .database_password(POSTGRES_PASSWORD.as_str())
        .database_host(POSTGRES_HOST.as_str())
        .database_port(*POSTGRES_PORT)
        .database_name(POSTGRES_DB.as_ref())
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
    let migrations = vec![make_migration!("20190806011140_initial-tables")];

    // Define Migrations
    config
        .use_migrations(&migrations)
        .expect("Loading migrations failed");

    // Reload config, ping the database for applied migrations
    let config = config.reload().expect("Should reload applied migrations");

    if let Environment::Development = environment {
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

#[cfg(any(test, feature = "test-util"))]
#[cfg_attr(docsrs, doc(cfg(feature = "test-util")))]
pub mod tests_postgres {
    use std::{
        ops::{Deref, DerefMut},
        sync::atomic::{AtomicUsize, Ordering},
    };

    use deadpool::managed::{Manager as ManagerTrait, RecycleError, RecycleResult};
    use deadpool_postgres::ManagerConfig;
    use once_cell::sync::Lazy;
    use primitives::postgres::POSTGRES_CONFIG;
    use tokio_postgres::{NoTls, SimpleQueryMessage};

    use async_trait::async_trait;
    use thiserror::Error;

    use super::{DbPool, PoolError};

    pub type Pool = deadpool::managed::Pool<Manager>;

    pub static DATABASE_POOL: Lazy<Pool> =
        Lazy::new(|| create_pool("test").expect("Should create test pool"));

    /// we must have a duplication of the migration because of how migrant is handling migrations
    /// we need to separately setup test migrations
    pub static MIGRATIONS: &[&str] = &["20190806011140_initial-tables"];

    fn create_pool(db_prefix: &str) -> Result<Pool, Error> {
        let manager_config = ManagerConfig {
            // to guarantee that `is_closed()` & test query will be ran to determine bad connections
            recycling_method: deadpool_postgres::RecyclingMethod::Verified,
        };
        let manager = Manager::new(POSTGRES_CONFIG.clone(), manager_config, db_prefix)?;

        Pool::builder(manager)
            .max_size(15)
            .build()
            .map_err(|err| match err {
                deadpool::managed::BuildError::Backend(err) => err,
                deadpool::managed::BuildError::NoRuntimeSpecified(message) => {
                    Error::Build(deadpool::managed::BuildError::NoRuntimeSpecified(message))
                }
            })
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

    /// Manager Error
    #[derive(Debug, Error)]
    pub enum Error {
        #[error(transparent)]
        Build(#[from] deadpool_postgres::BuildError),
        #[error(transparent)]
        Pool(#[from] PoolError),
    }

    impl From<tokio_postgres::Error> for Error {
        fn from(err: tokio_postgres::Error) -> Self {
            Error::Pool(PoolError::Backend(err))
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
        ) -> Result<Self, Error> {
            // We need to create the schema with a temporary connection, in order to use it for the real Test Pool
            let base_manager = deadpool_postgres::Manager::from_config(
                base_config.clone(),
                NoTls,
                manager_config.clone(),
            );
            let base_pool = deadpool_postgres::Pool::builder(base_manager)
                .max_size(15)
                .build()?;

            Ok(Self::new_with_pool(
                base_pool,
                base_config,
                manager_config,
                db_prefix,
            ))
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

        type Error = Error;

        async fn create(&self) -> Result<Self::Type, Self::Error> {
            let pool_index = self.index.fetch_add(1, Ordering::SeqCst);

            // e.g. test_0, test_1, test_2
            let db_name = format!("{}_{}", self.db_prefix, pool_index);

            // 1. Drop the database if it exists - if a test failed before, the database wouldn't have been removed
            // 2. Create database
            let drop_db = format!("DROP DATABASE IF EXISTS {0} WITH (FORCE);", db_name);
            let created_db = format!("CREATE DATABASE {0};", db_name);

            let temp_client = self.base_pool.get().await.map_err(|err| {
                match &err {
                    PoolError::Backend(backend_err) if backend_err.is_closed() => {
                        panic!("Closed PG Client connection of the base Pool!");
                    }
                    _ => {}
                }
                err
            })?;

            assert!(!self.base_pool.is_closed(), "Base Pool should never close");
            assert!(
                !self
                    .base_pool
                    .get()
                    .await
                    .expect("Should get connection")
                    .is_closed(),
                "a base pool connection should never be closed"
            );

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

            // TODO: Fix error mapping
            let pool = deadpool_postgres::Pool::builder(manager)
                .max_size(15)
                .build()
                .map_err(|err| match err {
                    deadpool::managed::BuildError::Backend(err) => PoolError::Backend(err),
                    deadpool::managed::BuildError::NoRuntimeSpecified(_err) => {
                        PoolError::NoRuntimeSpecified
                    }
                })?;

            // this will make sure the connection succeeds
            // Instead of making a connection the Pool returns directly.
            let _ = pool.get().await?;

            Ok(Database::new(db_name, pool))
        }

        async fn recycle(&self, database: &mut Database) -> RecycleResult<Self::Error> {
            // DROP the public schema and create it again for usage after recycling
            let queries = "DROP SCHEMA public CASCADE; CREATE SCHEMA public;";

            database.pool = {
                let mut config = self.base_config.clone();
                // set the database in the configuration of the inside Pool (used for tests)
                config.dbname(&database.name);

                let manager = deadpool_postgres::Manager::from_config(
                    config,
                    NoTls,
                    self.manager_config.clone(),
                );

                deadpool_postgres::Pool::builder(manager)
                    .max_size(15)
                    .build()
                    .map_err(|err| RecycleError::Backend(Error::Build(err)))?
            };

            let result = database
                .pool
                .get()
                .await
                .map_err(|err| RecycleError::Backend(Error::Pool(err)))?
                .simple_query(queries)
                .await
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
                use std::{env::current_dir, fs::read_to_string};

                let full_path = current_dir().unwrap();
                // it always starts in `sentry` folder because of the crate scope
                // even when it's in the workspace
                let mut file = full_path.parent().unwrap().to_path_buf();
                file.push(format!("sentry/migrations/{}/up.sql", migration));

                read_to_string(file).expect("File migration couldn't be read")
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
            let pool = create_pool("testing_pool").expect("Should create testing_pool");

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

#[cfg(any(test, feature = "test-util"))]
#[cfg_attr(docsrs, doc(cfg(feature = "test-util")))]
pub mod redis_pool {

    use dashmap::DashMap;
    use deadpool::managed::{Manager as ManagerTrait, RecycleResult};
    use thiserror::Error;

    use crate::db::redis_connection;
    use async_trait::async_trait;

    use once_cell::sync::Lazy;

    use super::*;

    /// Re-export [`redis::cmd`] for testing purposes
    pub use redis::cmd;

    pub type Pool = deadpool::managed::Pool<Manager>;

    pub static TESTS_POOL: Lazy<Pool> = Lazy::new(|| {
        Pool::builder(Manager::new())
            .max_size(Manager::CONNECTIONS.into())
            .build()
            .expect("Should build Pools for tests")
    });

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

        /// Flushing (`FLUSHDB`) is synchronous by default in Redis
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
                            redis_connection(format!("{}{}", Self::URL, record.key()))
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
            let mut connection = redis_connection(format!("{}{}", Self::URL, database.index))
                .await
                .expect("Should connect");
            // first flush the database
            // this avoids the problem of flushing after the DB is picked up again by the Pool
            let flush_result = Self::flush_db(&mut connection).await;
            // make the database available
            database.available = true;
            database.connection = connection;

            flush_result.expect("Should have flushed the redis DB successfully");

            Ok(())
        }
    }
}
