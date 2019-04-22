use hyper::{Body, Request, Response};
use hyper::header::{CONTENT_LENGTH, CONTENT_TYPE};

use crate::request::Path;

pub struct ChannelListHandler {

}

impl ChannelListHandler {
    pub async fn handle(path: Path, request: Request<Body>) -> Response<Body> {
        let found = "List channels";

        let response = Response::builder()
            .header(CONTENT_LENGTH, found.len() as u64)
            .header(CONTENT_TYPE, "text/plain")
            .status(200)
            .body(Body::from(found))
            .expect("Failed to construct the response");

        response
    }
}