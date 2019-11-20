#![deny(clippy::all)]
#![deny(rust_2018_idioms)]

use crate::chain::chain;
use crate::db::DbPool;
use crate::middleware::auth;
use crate::middleware::cors::{cors, Cors};
use hyper::{Body, Method, Request, Response, StatusCode};
use lazy_static::lazy_static;
use primitives::adapter::Adapter;
use primitives::Config;
use redis::aio::MultiplexedConnection;
use regex::Regex;
use routes::cfg::config;
use routes::channel::{create_channel, last_approved};
use slog::{error, Logger};
use std::collections::HashMap;

pub mod middleware {
    pub mod auth;
    pub mod channel;
    pub mod cors;
}

pub mod routes {
    pub mod cfg;
    pub mod channel;
}

pub mod access;
mod chain;
pub mod db;
pub mod event_reducer;

lazy_static! {
    static ref CHANNEL_GET_BY_ID: Regex =
        Regex::new(r"^/channel/0x([a-zA-Z0-9]{64})/?$").expect("The regex should be valid");
    static ref LAST_APPROVED_BY_CHANNEL_ID: Regex = Regex::new(r"^/channel/0x([a-zA-Z0-9]{64})/last-approved?$").expect("The regex should be valid");
    static ref CHANNEL_STATUS_BY_CHANNEL_ID: Regex = Regex::new(r"^/channel/0x([a-zA-Z0-9]{64})/status?$").expect("The regex should be valid");
    // @TODO define other regex routes
}

async fn config_middleware(req: Request<Body>) -> Result<Request<Body>, ResponseError> {
    Ok(req)
}

#[derive(Debug)]
pub struct RouteParams(Vec<String>);

impl RouteParams {
    pub fn get(&self, index: usize) -> Option<String> {
        self.0.get(index).map(ToOwned::to_owned)
    }

    pub fn index(&self, i: usize) -> String {
        self.0[i].clone()
    }
}

#[derive(Clone)]
pub struct Application<A: Adapter> {
    pub adapter: A,
    pub logger: Logger,
    pub redis: MultiplexedConnection,
    pub pool: DbPool,
    pub _clustered: bool,
    pub port: u16,
    pub config: Config,
    __secret: (),
}

impl<A: Adapter + 'static> Application<A> {
    pub fn new(
        adapter: A,
        config: Config,
        logger: Logger,
        redis: MultiplexedConnection,
        pool: DbPool,
        clustered: bool,
        port: u16,
    ) -> Self {
        Self {
            adapter,
            config,
            logger,
            redis,
            pool,
            _clustered: clustered,
            port,
            __secret: (),
        }
    }

    pub async fn handle_routing(&self, req: Request<Body>) -> Response<Body> {
        let headers = match cors(&req) {
            Some(Cors::Simple(headers)) => headers,
            // if we have a Preflight, just return the response directly
            Some(Cors::Preflight(response)) => return response,
            None => Default::default(),
        };

        let mut req = match auth::for_request(req, &self.adapter, self.redis.clone()).await {
            Ok(req) => req,
            Err(error) => {
                error!(&self.logger, "{}", &error; "module" => "middleware-auth");
                return map_response_error(ResponseError::BadRequest("invalid auth".into()));
            }
        };

        let mut response = match (req.uri().path(), req.method()) {
            ("/cfg", &Method::GET) => config(req, &self).await,
            ("/channel", &Method::POST) => create_channel(req, &self).await,
            ("/channel/list", &Method::GET) => Err(ResponseError::NotFound),
            // This is important becuase it prevents us from doing
            // expensive regex matching for routes without /channel
            (route, method) if route.starts_with("/channel") => {
                // example with
                // @TODO remove later
                // regex matching for routes with params
                if let (Some(caps), &Method::GET) =
                    (LAST_APPROVED_BY_CHANNEL_ID.captures(route), method)
                {
                    let param = RouteParams(vec![caps
                        .get(1)
                        .map_or("".to_string(), |m| m.as_str().to_string())]);
                    req.extensions_mut().insert(param);

                    // example with middleware
                    // @TODO remove later
                    let req = match chain(req, vec![config_middleware]).await {
                        Ok(req) => req,
                        Err(error) => {
                            return map_response_error(error);
                        }
                    };

                    last_approved(req, &self).await
                } else {
                    Err(ResponseError::NotFound)
                }
            }
            _ => Err(ResponseError::NotFound),
        }
        .unwrap_or_else(map_response_error);

        // extend the headers with the initial headers we have from CORS (if there are some)
        response.headers_mut().extend(headers);
        response
    }
}

#[derive(Debug)]
pub enum ResponseError {
    NotFound,
    BadRequest(String),
}

impl<T> From<T> for ResponseError
where
    T: std::error::Error + 'static,
{
    fn from(error: T) -> Self {
        // @TODO use a error proper logger?
        println!("{:#?}", error);
        ResponseError::BadRequest("Bad Request: try again later".into())
    }
}

pub fn map_response_error(error: ResponseError) -> Response<Body> {
    match error {
        ResponseError::NotFound => not_found(),
        ResponseError::BadRequest(e) => bad_response(e),
    }
}

pub fn not_found() -> Response<Body> {
    let mut response = Response::new(Body::from("Not found"));
    let status = response.status_mut();
    *status = StatusCode::NOT_FOUND;
    response
}

pub fn bad_response(response_body: String) -> Response<Body> {
    let mut error_response = HashMap::new();
    error_response.insert("error", response_body);

    let body = Body::from(serde_json::to_string(&error_response).expect("serialise err response"));

    let mut response = Response::new(body);
    response
        .headers_mut()
        .insert("Content-type", "application/json".parse().unwrap());

    let status = response.status_mut();
    *status = StatusCode::BAD_REQUEST;

    response
}

pub fn success_response(response_body: String) -> Response<Body> {
    let body = Body::from(response_body);

    let mut response = Response::new(body);
    response
        .headers_mut()
        .insert("Content-type", "application/json".parse().unwrap());

    let status = response.status_mut();
    *status = StatusCode::OK;

    response
}

// @TODO: Make pub(crate)
#[derive(Debug, Clone)]
pub struct Session {
    pub era: i64,
    pub uid: String,
    pub ip: Option<String>,
}
