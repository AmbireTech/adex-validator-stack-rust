#![deny(clippy::all)]
#![deny(rust_2018_idioms)]

use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Error, Request, Response, Server, StatusCode};
use primitives::adapter::Adapter;
use primitives::Config;
use slog::{error, info, Logger};

pub mod routes {
    pub mod channel;
}

pub mod access;
pub mod db;

pub struct Application<A: Adapter> {
    adapter: A,
    logger: slog::Logger,
    _clustered: bool,
    port: u16,
    config: Config,
}

impl<A: Adapter + 'static> Application<A> {
    pub fn new(adapter: A, config: Config, logger: Logger, clustered: bool, port: u16) -> Self {
        Self {
            adapter,
            config,
            logger,
            _clustered: clustered,
            port,
        }
    }

    pub async fn run(&self) {
        let addr = ([127, 0, 0, 1], self.port).into();
        info!(&self.logger, "Listening on port {}!", self.port);

        let make_service = make_service_fn(move |_| {
            let adapter_config = (self.adapter.clone(), self.config.clone());
            async move {
                Ok::<_, Error>(service_fn(move |req| {
                    let adapter_config = adapter_config.clone();
                    async move { Ok::<_, Error>(handle_routing(req, adapter_config.0).await) }
                }))
            }
        });

        let server = Server::bind(&addr).serve(make_service);

        if let Err(e) = server.await {
            error!(&self.logger, "server error: {}", e);
        }
    }
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

async fn handle_routing(req: Request<Body>, adapter: impl Adapter) -> Response<Body> {
    if req.uri().path().starts_with("/channel") {
        crate::routes::channel::handle_channel_routes(req, adapter).await
    } else {
        Err(ResponseError::NotFound)
    }
    .unwrap_or_else(|response_err| match response_err {
        ResponseError::NotFound => not_found(),
        ResponseError::BadRequest(error) => bad_request(error),
    })
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

// @TODO: Make pub(crate)
#[derive(Debug, Clone)]
pub struct Session {
    pub era: i64,
    pub uid: String,
    pub ip: String,
}
