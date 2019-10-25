use futures::compat::Future01CompatExt;
use lazy_static::lazy_static;
use redis::aio::Connection;
use redis::RedisError;

lazy_static! {
    static ref REDIS_URL: String =
        std::env::var("REDIS_URL").unwrap_or_else(|_| String::from("redis://127.0.0.1:6379"));
}

pub async fn redis_connection() -> Result<Connection, RedisError> {
    let client = redis::Client::open(REDIS_URL.as_str()).unwrap();
    client.get_async_connection().compat().await
}
