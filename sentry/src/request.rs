use hyper::{Body, Method, Request, Response};
use hyper::header::{CONTENT_LENGTH, CONTENT_TYPE};
use regex::Regex;
use tokio::await;

use crate::application::error::ApplicationError;
use crate::database::channel::PostgresChannelRepository;
use crate::domain::Channel;
use crate::handler::channel::ChannelListHandler;
use crate::database::DbPool;

pub struct Path {
    matcher: Regex,
    pub path: String,
    pub method: Method,
}

impl Path {
    pub fn new(method: Method, path: &str) -> Path {
        let mut regex = "^".to_string();
        regex.push_str(path);
        regex.push_str("$");
        Path {
            matcher: Regex::new(&regex).unwrap(),
            path: regex,
            method,
        }
    }

    pub fn is_match(&self, method: Method, path: &str) -> bool {
        self.method == method && self.matcher.is_match(path)
    }
}

pub enum SentryRequest {
    ChannelList,
    ChannelCreate(Channel),
    ChannelRequest,
}

impl SentryRequest {
    pub async fn from_request(db_pool: DbPool, request: Request<Body>) -> Result<Response<Body>, ApplicationError> {
        let path = Path::new(request.method().clone(), request.uri().path());

        let channel_repository = PostgresChannelRepository::new(db_pool.clone());

        if path.is_match(Method::GET, "/channel/list") {
            let channel_list_handler = ChannelListHandler::new(&channel_repository);

            return Ok(await!(channel_list_handler.handle(path, request)));
        }

        let not_found = "404 Not found";
        Ok(Response::builder()
            .header(CONTENT_LENGTH, not_found.len() as u64)
            .header(CONTENT_TYPE, "text/plain")
            .status(404)
            .body(Body::from(not_found))
            .expect("Failed to construct the response"))
    }
}