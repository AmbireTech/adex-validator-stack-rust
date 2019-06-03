pub mod channel;
pub mod memory;
pub mod postgres;

pub type DbPool = bb8::Pool<bb8_postgres::PostgresConnectionManager<tokio_postgres::NoTls>>;

// @TODO: Find a way to define the repository in `ChannelResource::new()` and use it inside the `impl_web!` macro
pub enum Persistence {
    Memory,
    Postgres(DbPool),
}
