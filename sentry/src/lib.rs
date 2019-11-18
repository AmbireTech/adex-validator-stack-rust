#![deny(clippy::all)]
#![deny(rust_2018_idioms)]

use crate::db::DbPool;
use crate::middleware::auth;
use crate::middleware::cors::{cors, Cors};
use hyper::{Body, Method, Request, Response, StatusCode};
use primitives::adapter::Adapter;
use primitives::Config;
use redis::aio::MultiplexedConnection;
use slog::{error, Logger};
use lazy_static::lazy_static;
use regex::Regex;
use crate::chain::chain;


pub mod middleware {
    pub mod auth;
    pub mod channel;
    pub mod cors;
}

pub mod routes {
    pub mod channel;
    pub mod cfg;
}

pub mod access;
pub mod db;
pub mod event_reducer;
mod chain;

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
    pub  _clustered: bool,
    pub port: u16,
    pub config: Config,
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
        }
    }

    pub async fn handle_routing(
        &self,
        req: Request<Body>
    ) -> Response<Body> {
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
                return map_response_error(ResponseError::BadRequest(error));
            }
        };
    
        // req.uri().path() == "/channel" && req.method() == Method::POST 
        // if let (Some(caps), &Method::GET) = (CHANNEL_GET_BY_ID.captures(req.uri().path()), req.method())

        let config_controller = routes::cfg::ConfigController::new(&self);
        let channel_controller = routes::channel::ChannelController::new(&self);

    
        let mut response = match (req.uri().path(), req.method()) {
            ("/cfg", &Method::GET) => config_controller.config(req).await,
            ("/channel", &Method::POST) => {
                // example with middleware
                // @TODO remove later
                let req = match chain(req, vec![config_middleware]).await {
                    Ok(req) => req,
                    Err(error) => {
                        return map_response_error(error);
                    }
                };
                config_controller.config(req).await
            },
            ("/channel/list", &Method::GET) => Err(ResponseError::NotFound),
            (route, method) if route.starts_with("/channel") => {
                // example with 
                // @TODO remove later
                // regex matching for routes with params
                if let (Some(caps), &Method::GET) = (LAST_APPROVED_BY_CHANNEL_ID.captures(route), method) {
                    let param = RouteParams(vec![ caps.get(1).map_or("".to_string(), |m| m.as_str().to_string()) ]);
                    req.extensions_mut().insert(param);
                    channel_controller.last_approved(req).await
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

pub fn bad_request(err: Box<dyn std::error::Error>) -> Response<Body> {
    println!("{:#?}", err);
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
