use bb8::Pool;
use bb8_postgres::tokio_postgres::NoTls;
use bb8_postgres::PostgresConnectionManager;
use redis::aio::MultiplexedConnection;
use redis::RedisError;
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
    static ref REDIS_URL: String =
        env::var("REDIS_URL").unwrap_or_else(|_| String::from("redis://127.0.0.1:6379"));
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

pub async fn redis_connection() -> Result<MultiplexedConnection, RedisError> {
    let client = redis::Client::open(REDIS_URL.as_str())?;
    let (connection, driver) = client.get_multiplexed_async_connection().await?;
    tokio::spawn(driver);
    Ok(connection)
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

    let mut migrations = vec![
        make_migration!("20190806011140_initial-tables"),
        make_migration!("20200625092729_channel-targeting-rules"),
    ];

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
