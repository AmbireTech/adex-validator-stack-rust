use redis::aio::MultiplexedConnection;
use redis::RedisError;

use lazy_static::lazy_static;
use bb8_postgres::PostgresConnectionManager;
use bb8_postgres::tokio_postgres::NoTls;
use bb8::Pool;

pub type DbPool = Pool<PostgresConnectionManager<NoTls>>;

lazy_static! {
    static ref REDIS_URL: String =
        std::env::var("REDIS_URL").unwrap_or_else(|_| String::from("redis://127.0.0.1:6379"));
    static ref POSTGRES_URL: String = std::env::var("POSTGRES_URL")
        .unwrap_or_else(|_| String::from("postgresql://postgres:postgres@localhost:5432"));
}

pub async fn redis_connection() -> Result<MultiplexedConnection, RedisError> {
    let client = redis::Client::open(REDIS_URL.as_str()).expect("Wrong redis connection string");
    client.get_multiplexed_tokio_connection().await
}

pub async fn postgres_connection() -> Result<DbPool, bb8_postgres::tokio_postgres::Error>
{
    let pg_mgr = PostgresConnectionManager::new_from_stringlike(POSTGRES_URL.as_str(), NoTls)?;

    Pool::builder().build(pg_mgr).await
}
