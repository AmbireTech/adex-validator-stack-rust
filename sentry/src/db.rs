use bb8::Pool;
use bb8_postgres::tokio_postgres::NoTls;
use bb8_postgres::PostgresConnectionManager;
use redis::aio::MultiplexedConnection;
use redis::RedisError;
use std::env;

use lazy_static::lazy_static;

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
    let client = redis::Client::open(REDIS_URL.as_str()).expect("Wrong redis connection url");
    client.get_multiplexed_tokio_connection().await
}

pub async fn postgres_connection() -> Result<DbPool, bb8_postgres::tokio_postgres::Error> {
    let mut config = bb8_postgres::tokio_postgres::Config::new();

    config
        .user(POSTGRES_USER.as_str())
        .password(POSTGRES_PASSWORD.as_str())
        .host(POSTGRES_HOST.as_str())
        .port(POSTGRES_PORT.clone());
    if let Some(db) = POSTGRES_DB.clone() {
        config.dbname(&db);
    }
    let pg_mgr = PostgresConnectionManager::new(config, NoTls);

    Pool::builder().build(pg_mgr).await
}

pub async fn setup_migrations() {
    use migrant_lib::{Config, Direction, Migrator, Settings};

    let settings = Settings::configure_postgres()
        .database_user(POSTGRES_USER.as_str())
        .database_password(POSTGRES_PASSWORD.as_str())
        .database_host(POSTGRES_HOST.as_str())
        .database_port(POSTGRES_PORT.clone())
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

    // Define Migrations
    config
        .use_migrations(&[make_migration!("20190806011140_initial_tables")])
        .expect("Loading migrations failed");

    // Reload config, ping the database for applied migrations
    let config = config.reload().expect("Should reload applied migrations");

    Migrator::with_config(&config)
        // set `swallow_completion` to `true`
        // so no error will be returned if all migrations have already been ran
        .swallow_completion(true)
        .direction(Direction::Up)
        .all(true)
        .apply()
        .expect("Applying migrations failed");

    let _config = config
        .reload()
        .expect("Reloading config for migration failed");
}
