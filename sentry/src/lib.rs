#![deny(clippy::all)]
#![deny(rust_2018_idioms)]

use crate::chain::chain;
use crate::middleware::auth;
use crate::middleware::cors::{cors, Cors};
use bb8::Pool;
use bb8_postgres::{tokio_postgres::NoTls, PostgresConnectionManager};
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Error, Method, Request, Response, Server, StatusCode};
use primitives::adapter::Adapter;
use primitives::Config;
use redis::aio::MultiplexedConnection;
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
        use hyper::{Body, Request, Response};
        use primitives::Config;

        pub async fn return_config(req: Request<Body>) -> Result<Response<Body>, ResponseError> {
            let config = req
                .extensions()
                .get::<Config>()
                .expect("request should have config");
            let config_str = serde_json::to_string(config)?;

            Ok(Response::builder()
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(config_str))
                .expect("Creating a response should never fail"))
        }
    }
}

pub mod access;
mod chain;
pub mod db;
pub mod event_reducer;

pub struct Application<A: Adapter> {
    adapter: A,
    logger: Logger,
    redis: MultiplexedConnection,
    _postgres: Pool<PostgresConnectionManager<NoTls>>,
    _clustered: bool,
    port: u16,
    config: Config,
}

impl<A: Adapter + 'static> Application<A> {
    pub fn new(
        adapter: A,
        config: Config,
        logger: Logger,
        redis: MultiplexedConnection,
        postgres: Pool<PostgresConnectionManager<NoTls>>,
        clustered: bool,
        port: u16,
    ) -> Self {
        Self {
            adapter,
            config,
            logger,
            redis,
            _postgres: postgres,
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
            let logger = self.logger.clone();
            async move {
                Ok::<_, Error>(service_fn(move |req| {
                    let adapter_config = adapter_config.clone();
                    let redis = redis.clone();
                    let logger = logger.clone();
                    async move {
                        Ok::<_, Error>(
                            handle_routing(
                                req,
                                (&adapter_config.0, &adapter_config.1),
                                redis,
                                &logger,
                            )
                            .await,
                        )
                    }
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
    BadRequest,
}

impl<T> From<T> for ResponseError
where
    T: std::error::Error + 'static,
{
    fn from(_: T) -> Self {
        ResponseError::BadRequest
    }
}

async fn config_middleware(req: Request<Body>) -> Result<Request<Body>, ResponseError> {
    Ok(req)
}

async fn handle_routing(
    mut req: Request<Body>,
    (adapter, config): (&impl Adapter, &Config),
    redis: MultiplexedConnection,
    logger: &Logger,
) -> Response<Body> {
    req.extensions_mut().insert(config.clone());

    let headers = match cors(&req) {
        Some(Cors::Simple(headers)) => headers,
        // if we have a Preflight, just return the response directly
        Some(Cors::Preflight(response)) => return response,
        None => Default::default(),
    };

    let req = match auth::for_request(req, adapter, redis.clone()).await {
        Ok(req) => req,
        Err(error) => {
            error!(&logger, "{}", &error; "module" => "middleware-auth");

            return map_response_error(ResponseError::BadRequest);
        }
    };

    let mut response = match (req.uri().path(), req.method()) {
        ("/cfg", &Method::GET) => {
            chain(
                req,
                Some(vec![config_middleware]),
                crate::routes::cfg::return_config,
            )
            .await
        }
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
        ResponseError::BadRequest => bad_request(),
    }
}

pub fn not_found() -> Response<Body> {
    let mut response = Response::new(Body::from("Not found"));
    let status = response.status_mut();
    *status = StatusCode::NOT_FOUND;
    response
}

pub fn bad_request() -> Response<Body> {
    let body = Body::from("Bad Request: try again later");
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
