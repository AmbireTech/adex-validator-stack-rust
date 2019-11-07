#![deny(clippy::all)]
#![deny(rust_2018_idioms)]

use crate::middleware::auth;
use crate::middleware::cors::{cors, CorsResult};
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Error, Method, Request, Response, Server, StatusCode};
use primitives::adapter::Adapter;
use primitives::Config;
use redis::aio::SharedConnection;
use slog::{error, info, Logger};

pub mod middleware {
    pub mod auth;
    pub mod cors;
}
pub mod routes {
    pub mod channel;
    pub mod cfg {
        use crate::ResponseError;
        use hyper::header::CONTENT_TYPE;
        use hyper::{Body, Response};
        use primitives::Config;

        pub fn return_config(config: &Config) -> Result<Response<Body>, ResponseError> {
            let config_str = serde_json::to_string(config)?;

            Ok(Response::builder()
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(config_str))
                .expect("Creating a response should never fail"))
        }
    }
}

pub mod access;
pub mod db;
pub mod event_reducer;

pub struct Application<A: Adapter> {
    adapter: A,
    logger: slog::Logger,
    redis: SharedConnection,
    _clustered: bool,
    port: u16,
    config: Config,
}

impl<A: Adapter + 'static> Application<A> {
    pub fn new(
        adapter: A,
        config: Config,
        logger: Logger,
        redis: SharedConnection,
        clustered: bool,
        port: u16,
    ) -> Self {
        Self {
            adapter,
            config,
            logger,
            redis,
            _clustered: clustered,
            port,
        }
    }

    /// Starts the `hyper` `Server`.
    pub async fn run(&self) {
        let addr = ([127, 0, 0, 1], self.port).into();
        info!(&self.logger, "Listening on port {}!", self.port);

        let make_service = make_service_fn(move |_| {
            let adapter_config = (self.adapter.clone(), self.config.clone());
            let redis = self.redis.clone();
            async move {
                Ok::<_, Error>(service_fn(move |req| {
                    let adapter_config = adapter_config.clone();
                    let redis = redis.clone();
                    async move { Ok::<_, Error>(handle_routing(req, adapter_config, redis).await) }
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

async fn handle_routing(
    req: Request<Body>,
    (adapter, config): (impl Adapter, Config),
    redis: SharedConnection,
) -> Response<Body> {
    let headers = match cors(&req) {
        CorsResult::Simple(headers) => headers,
        // if we have a Preflight, just return the response directly
        CorsResult::Preflight(response) => return response,
        CorsResult::None => Default::default(),
    };

    // otherwise problems with `.await` occurs about `Sync` being required for `Adapter`.
    let auth_connections = (adapter.clone(), redis.clone());
    let req = match auth::for_request(req, auth_connections.0, auth_connections.1).await {
        Ok(req) => req,
        Err(response_error) => return map_response_error(response_error),
    };

    let mut response = match (req.uri().path(), req.method()) {
        ("/cfg", &Method::GET) => crate::routes::cfg::return_config(&config),
        (route, _) if route.starts_with("/channel") => {
            crate::routes::channel::handle_channel_routes(req, adapter).await
        }
        _ => Err(ResponseError::NotFound),
    }
    .unwrap_or_else(map_response_error);

    // extend the headers with the initial headers we have from CORS (if there are some)
    response.headers_mut().extend(headers);
    response
}

fn map_response_error(error: ResponseError) -> Response<Body> {
    match error {
        ResponseError::NotFound => not_found(),
        ResponseError::BadRequest(error) => bad_request(error),
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

// @TODO: Make pub(crate)
#[derive(Debug, Clone)]
pub struct Session {
    pub era: i64,
    pub uid: String,
    pub ip: Option<String>,
}
