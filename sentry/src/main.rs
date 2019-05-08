#![feature(async_await, await_macro)]

use std::net::SocketAddr;

use futures::future::{FutureExt, TryFutureExt};
use futures_legacy::Future;
use futures_legacy::lazy as old_lazy;
use tokio::await;
use tokio_tcp::TcpListener;
use tower_web::ServiceBuilder;

use sentry::application::resource::channel::ChannelResource;

const DEFAULT_PORT: u16 = 8005;

pub fn main() {
    // @TODO: Define and use a CLI for setting sentry options

    let port: u16 = std::env::var("PORT")
        .unwrap_or_else(|_| format!("{}", DEFAULT_PORT))
        .parse()
        .expect("Failed to parse port");
    let database_url = std::env::var("DATABASE_URL").expect("Missing DATABASE_URL");

    let addr = SocketAddr::from(([0, 0, 0, 0], port));

    println!("Listening on http://{}", addr);

    tokio::run(old_lazy(move || bootstrap(database_url, addr).unit_error().boxed().compat()))
}

async fn bootstrap(database_url: String, addr: SocketAddr) {
    let db_pool = await!(database_pool(database_url)).unwrap();

    let listener = TcpListener::bind(&addr).expect("Wrong address provided");

    // A service builder is used to configure our service.
    let server = ServiceBuilder::new()
        .resource(ChannelResource { db_pool: db_pool.clone() })
        .serve(listener.incoming());

    await!(server).expect("Server error");
}

fn database_pool(database_url: String) -> impl Future<Item=bb8::Pool<bb8_postgres::PostgresConnectionManager<tokio_postgres::NoTls>>, Error=tokio_postgres::Error> {
    let postgres_connection = bb8_postgres::PostgresConnectionManager::new(
        database_url,
        tokio_postgres::NoTls,
    );

    bb8::Pool::builder().build(postgres_connection)
}

