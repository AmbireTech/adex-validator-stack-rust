#![feature(async_await, await_macro, futures_api)]
use std::net::SocketAddr;
use futures::future::{FutureExt, TryFutureExt};
use futures_legacy::future::Future;
use futures_legacy::stream::Stream;
use hyper::{Body, Request, Response};
use hyper::header::{CONTENT_LENGTH, CONTENT_TYPE};
use hyper::server::Server;
use hyper::service::service_fn;
use tokio::await;

const DEFAULT_PORT: u16 = 8005;

async fn serve_req(req: Request<Body>) -> Result<Response<Body>, hyper::Error> {

    let body = "Sample";
    Ok(Response::builder()
        .header(CONTENT_LENGTH, body.len() as u64)
        .header(CONTENT_TYPE, "text/plain")
        .body(Body::from(body))
        .expect("Failed to construct the response"))
}

async fn run_server(addr: SocketAddr) {
    println!("Listening on http://{}", addr);

    let serve_future =
        Server::bind(&addr).serve(|| service_fn(|req| serve_req(req).boxed().compat()));

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