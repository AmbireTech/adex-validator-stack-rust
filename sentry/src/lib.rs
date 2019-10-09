#![deny(clippy::all)]
#![deny(rust_2018_idioms)]

use hyper::{Body, Response, StatusCode};
use primitives::adapter::Adapter;
use std::error::Error;

pub mod routes {
    pub mod channel;
}

pub struct Application<T: Adapter> {
    // database to be initialised
    // storage: Storage,
    adapter: T,
    logger: slog::Logger,
}

impl<T: Adapter> Application<T> {
    fn new() -> Self {
        unimplemented!("whoopsy")
    }
}

pub fn not_found() -> Response<Body> {
    let mut response = Response::new(Body::from("Not found"));
    let status = response.status_mut();
    *status = StatusCode::NOT_FOUND;
    response
}

pub fn bad_request(error: Option<Box<dyn std::error::Error>>) -> Response<Body> {
    let body = match error {
        Some(err) => Body::from(format!("Bad Request: {}", error)),
        None => Body::empty(),
    };
    let mut response = Response::new(body);
    let status = response.status_mut();
    *status = StatusCode::BAD_REQUEST;
    response
}
