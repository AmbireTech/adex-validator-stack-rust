use futures::Future;
use futures::future::ok;
use hyper::{Body, Method, Request, Response};
use hyper::header::{CONTENT_LENGTH, CONTENT_TYPE};
use regex::Regex;
use tokio::await;
use futures::future::{FutureExt, TryFutureExt};

use crate::domain::Channel;

pub struct Path {
    matcher: Regex,
    method: Method,
}

impl Path {
    pub fn new(method: Method, path: &str) -> Path {
        let mut regex = "^".to_string();
        regex.push_str(path);
        regex.push_str("$");
        Path {
            matcher: Regex::new(&regex).unwrap(),
            method
        }
    }

    pub fn is_match(&self, method: &Method, path: &str) -> bool {
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
    pub async fn from_request(request: Request<Body>) -> Result<Response<Body>, hyper::Error> {
        let routes = vec![
            ("/channel/list".to_string(), Method::GET, channel_list),
        ];

        let found_route = routes.iter().find_map(|(ref path_str, ref method, handler)| {
            let path = Path::new(method.clone(), path_str);

            let uri = request.uri().path_and_query().unwrap();
            if path.is_match(request.method(), uri.path()) {
                Some((path, *handler))
            } else {
                None
            }
        });

        let response = match found_route {
            Some((path, handler)) => {
                let fut_response = handler(path, request);

                let response: Response<Body> = await!(fut_response);
                response

//                Response::builder()
//                    .header(CONTENT_LENGTH, 3 as u64)
//                    .header(CONTENT_TYPE, "text/plain")
//                    .status(200)
//                    .body(Body::from("abs"))
//                    .expect("Failed to construct the response")
            },
            None => {
                let not_found = "404 Not found";
                Response::builder()
                    .header(CONTENT_LENGTH, not_found.len() as u64)
                    .header(CONTENT_TYPE, "text/plain")
                    .status(404)
                    .body(Body::from(not_found))
                    .expect("Failed to construct the response")
            }
        };

        Ok(response)
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