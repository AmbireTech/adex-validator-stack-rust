use hyper::{Body, Request, Response};
use hyper::header::{CONTENT_LENGTH, CONTENT_TYPE};
use tokio::await;

use crate::database::channel::PostgresChannelRepository;
use crate::request::Path;

pub struct ChannelListHandler<'a> {
    channel_repository: &'a PostgresChannelRepository,
}

impl<'a> ChannelListHandler<'a> {
    pub fn new(channel_repository: &'a PostgresChannelRepository) -> Self {
        Self { channel_repository }
    }
}

impl<'a> ChannelListHandler<'a> {
    pub async fn handle(&self, _path: Path, _request: Request<Body>) -> Response<Body> {
        let channels = await!(self.channel_repository.list_as()).unwrap();
        let json = serde_json::to_string(&channels).unwrap();

        let response = Response::builder()
            .header(CONTENT_LENGTH, json.len() as u64)
            .header(CONTENT_TYPE, "text/plain")
            .status(200)
            .body(Body::from(json))
            .expect("Failed to construct the response");

        response
    }
}