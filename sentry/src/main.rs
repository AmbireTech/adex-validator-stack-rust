#![feature(async_await, await_macro, futures_api)]

use std::net::SocketAddr;

use futures::future::{FutureExt, TryFutureExt};
use hyper::server::Server;
use hyper::service::service_fn;
use postgres::{Client, NoTls};
use tokio::await;

use sentry::request::SentryRequest;

const DEFAULT_PORT: u16 = 8005;

async fn run_server(addr: SocketAddr) {
    println!("Listening on http://{}", addr);

    let serve_future =
        Server::bind(&addr).serve(|| service_fn(|req| {
            let client = Client::connect("host=localhost user=postgres password=docker dbname=sentry", NoTls).unwrap();

            SentryRequest::from_request(client, req).boxed().compat()
        }));

    if let Err(e) = await!(serve_future) {
        eprintln!("server error: {}", e);
    }
}

fn main() {
    // @TODO: Define and use a CLI for setting sentry options

    // Set the address to run our socket on.
    let addr = SocketAddr::from(([0, 0, 0, 0], DEFAULT_PORT));

    let fut = run_server(addr);

    hyper::rt::run(fut.unit_error().boxed().compat());
}