use futures::compat::Future01CompatExt;
use redis::aio::SharedConnection;
use redis::RedisError;

use lazy_static::lazy_static;

lazy_static! {
    static ref REDIS_URL: String =
        std::env::var("REDIS_URL").unwrap_or_else(|_| String::from("redis://127.0.0.1:6379"));
}

pub async fn redis_connection() -> Result<SharedConnection, RedisError> {
    let client = redis::Client::open(REDIS_URL.as_str()).expect("Wrong redis connection string");
    client.get_shared_async_connection().compat().await
}
