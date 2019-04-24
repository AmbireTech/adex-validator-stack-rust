use futures::future::{Future, FutureExt, TryFutureExt};
use futures::future::ok;
use futures_legacy::future::Future as LegacyFuture;
use hyper::{Body, Method, Request, Response};
use hyper::header::{CONTENT_LENGTH, CONTENT_TYPE};
use postgres::Client;
use regex::Regex;
use tokio::await;

use crate::database::channel::PostgresChannelRepository;
use crate::domain::Channel;
use crate::handler::channel::ChannelListHandler;

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
        if self.method == method && self.matcher.is_match(path) {
            true
        } else {
            false
        }
    }
}

pub enum SentryRequest {
    ChannelList,
    ChannelCreate(Channel),
    ChannelRequest,
}

impl SentryRequest {
    pub async fn from_request(mut client: Client, request: Request<Body>) -> Result<Response<Body>, hyper::Error> {

        // @TODO: handle error
        let path_and_query = request.uri().path_and_query().unwrap();
        let path = Path::new(request.method().clone(), path_and_query.path());

        if path.is_match(Method::GET, "/channel/list") {
            let mut channel_repository = PostgresChannelRepository::new(&mut client);
            let mut channel_list_handler = ChannelListHandler::new(&mut channel_repository);

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

pub fn channel_list(path: Path, request: Request<Body>) -> Box<Future<Output=Result<Response<Body>, hyper::Error>>> {
    let not_found = "Found";

    let response = Response::builder()
        .header(CONTENT_LENGTH, not_found.len() as u64)
        .header(CONTENT_TYPE, "text/plain")
        .status(200)
        .body(Body::from(not_found))
        .expect("Failed to construct the response");

    Box::new(ok(response))
}