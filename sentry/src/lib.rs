#![deny(clippy::all)]
#![deny(rust_2018_idioms)]

use hyper::{Body, Response, StatusCode};

pub mod routes {
    pub mod channel;
}

#[derive(Debug)]
pub enum ResponseError {
    NotFound,
    BadRequest(Box<dyn std::error::Error>),
}

impl<T> From<T> for ResponseError
where
    T: std::error::Error + 'static,
{
    fn from(error: T) -> Self {
        ResponseError::BadRequest(error.into())
    }
}

pub fn not_found() -> Response<Body> {
    let mut response = Response::new(Body::from("Not found"));
    let status = response.status_mut();
    *status = StatusCode::NOT_FOUND;
    response
}

pub fn bad_request(error: Box<dyn std::error::Error>) -> Response<Body> {
    let body = Body::from(format!("Bad Request: {}", error));
    let mut response = Response::new(body);
    let status = response.status_mut();
    *status = StatusCode::BAD_REQUEST;
    response
}
