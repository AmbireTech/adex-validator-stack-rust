#![feature(async_await, await_macro)]

use std::convert::TryFrom;
use std::env::Vars;
use std::net::SocketAddr;

use futures::compat::Future01CompatExt;
use futures::future::{FutureExt, TryFutureExt};
use tokio::await;
use tokio_tcp::TcpListener;
use tower_web::ServiceBuilder;

use domain::DomainError;
use lazy_static::lazy_static;
use sentry::application::resource::channel::ChannelResource;
use sentry::infrastructure::persistence::channel::{
    MemoryChannelRepository, PostgresChannelRepository,
};
use sentry::infrastructure::persistence::DbPool;
use std::sync::Arc;

const DEFAULT_PORT: u16 = 8005;

lazy_static! {
    static ref CONFIG: Config = {
        dotenv::dotenv().ok();
        Config::try_from(std::env::vars()).expect("Config failed")
    };
}

pub fn main() {
    // @TODO: Define and use a CLI for setting sentry options

    let port: u16 = std::env::var("PORT")
        .unwrap_or_else(|_| format!("{}", DEFAULT_PORT))
        .parse()
        .expect("Failed to parse port");
    let database_url = std::env::var("DATABASE_URL").expect("Missing DATABASE_URL");

    let addr = SocketAddr::from(([0, 0, 0, 0], port));

    println!("Listening on http://{}", addr);

    tokio::run(bootstrap(database_url, addr).unit_error().boxed().compat())
}

async fn bootstrap(database_url: String, addr: SocketAddr) {
    // @TODO: Error handling
    let db_pool = await!(database_pool(database_url)).expect("Database connection failed");

    let listener = TcpListener::bind(&addr).expect("Wrong address provided");

    let channel_repository = Arc::new(MemoryChannelRepository::new(None));
    let _channel_repository = Arc::new(PostgresChannelRepository::new(db_pool.clone()));

    // A service builder is used to configure our service.
    let server = ServiceBuilder::new()
        .resource(ChannelResource {
            channel_list_limit: CONFIG.channel_list_limit,
            channel_repository: channel_repository.clone(),
        })
        .serve(listener.incoming());

    await!(server).expect("Server error");
}

async fn database_pool(database_url: String) -> Result<DbPool, tokio_postgres::Error> {
    let postgres_connection =
        bb8_postgres::PostgresConnectionManager::new(database_url, tokio_postgres::NoTls);

    await!(bb8::Pool::builder().build(postgres_connection).compat())
}

#[derive(Debug, Clone, Copy)]
struct Config {
    pub channel_list_limit: u32,
}

impl TryFrom<Vars> for Config {
    type Error = domain::DomainError;

    fn try_from(mut vars: Vars) -> Result<Self, Self::Error> {
        let limit = vars
            .find_map(|(key, value)| {
                if key == "SENTRY_CHANNEL_LIST_LIMIT" {
                    Some(value)
                } else {
                    None
                }
            })
            .ok_or(DomainError::InvalidArgument(
                "SENTRY_CHANNEL_LIST_LIMIT evn. variable was not passed".to_string(),
            ))
            .and_then(|value| {
                value.parse::<u32>().map_err(|_| {
                    DomainError::InvalidArgument(
                        "SENTRY_CHANNEL_LIST_LIMIT is not a u32 value".to_string(),
                    )
                })
            });

        Ok(Self {
            channel_list_limit: limit?,
        })
    }
}
