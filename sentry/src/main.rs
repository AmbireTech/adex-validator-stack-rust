#![feature(async_await, await_macro)]

use std::net::SocketAddr;

use futures::future::{FutureExt, TryFutureExt};
use futures::compat::Future01CompatExt;
use futures_legacy::future::IntoFuture;
use tokio::await;
use futures_legacy::Future as OldFuture;
use try_future::try_future;

use sentry::application::request::SentryRequest;
use sentry::application::request::request_router;
use sentry::application::error::ApplicationError;

const DEFAULT_PORT: u16 = 8005;

fn main() {
    // @TODO: Define and use a CLI for setting sentry options

    let port: u16 = std::env::var("PORT")
        .unwrap_or_else(|_| format!("{}", DEFAULT_PORT) )
        .parse()
        .expect("Failed to parse port");
    let database_url = std::env::var("DATABASE_URL").expect("Missing DATABASE_URL");

    let addr = SocketAddr::from(([0, 0, 0, 0], port));

    tokio::run(bootstrap(database_url, addr).unit_error().boxed().compat() )
}

async fn bootstrap(database_url: String, addr: SocketAddr) {
    let db_pool = await!(database_pool(database_url)).expect("Database error");

    // create a service that will wrap the clone of the DB Pool and then serves the request
    let make_service = hyper::service::make_service_fn(
        move |addr_stream: &hyper::server::conn::AddrStream| {
            // @TODO use [enclose!](https://crates.io/crates/enclose)
            let db_pool = db_pool.clone();
            let ip_addr = addr_stream.remote_addr();

            // create a service fn that will handle the request once we have the cloned pool
            hyper::service::service_fn(move |req| {
                request_entrypoint(req, db_pool.clone(), ip_addr).boxed().compat()
            })
        },
    );

    println!("Listening on http://{}", addr);

    // serve the hyper server
    let serve_fut = hyper::Server::bind(&addr).serve(make_service);

    await!(serve_fut).expect("Server error");
}

fn database_pool(database_url: String) -> impl OldFuture<Item=bb8::Pool<bb8_postgres::PostgresConnectionManager<tokio_postgres::NoTls>>, Error=tokio_postgres::Error> {
    let postgres_connection = bb8_postgres::PostgresConnectionManager::new(
        database_url,
        tokio_postgres::NoTls,
    );

    bb8::Pool::builder().build(postgres_connection)
}

pub async fn request_entrypoint(
    request: hyper::Request<hyper::Body>,
    db_pool: sentry::infrastructure::persistence::DbPool,
    _addr: std::net::SocketAddr,
) -> Result<hyper::Response<hyper::Body>, http::Error> {
    let fut = request.headers()
        .get(hyper::header::HOST)
        .cloned()
        .ok_or(ApplicationError::NoHost)
        .into_future()
        .and_then(move |host| {
            let _host: String = try_future!(host.to_str().map_err(|_| ApplicationError::InvalidHostValue).map(|s| s.to_owned()));
            // into() will make it into TryFuture
            request_router(request).boxed().compat().into()
        })
        .and_then(|sentry_request| SentryRequest::handle(db_pool, sentry_request).boxed().compat())
        .or_else(|err| err.as_response())
        .compat();

    await!(fut)
}

