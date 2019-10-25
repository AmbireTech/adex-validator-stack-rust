use lazy_static::lazy_static;

lazy_static! {
    static ref REDIS_URL: String =
        std::env::var("REDIS_URL").unwrap_or_else(|_| String::from("redis://127.0.0.1:6379"));
}
