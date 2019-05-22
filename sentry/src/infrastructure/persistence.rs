pub mod channel;
pub mod memory;
pub mod postgres;

pub type DbPool = bb8::Pool<bb8_postgres::PostgresConnectionManager<tokio_postgres::NoTls>>;

pub enum Persistence {
    Memory,
    Postgres(DbPool),
}