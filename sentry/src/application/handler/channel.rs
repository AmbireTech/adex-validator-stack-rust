use hyper::{Body, Response};
use hyper::header::{CONTENT_LENGTH, CONTENT_TYPE};
use tokio::await;

use crate::infrastructure::persistence::channel::PostgresChannelRepository;

pub struct ChannelListHandler<'a> {
    channel_repository: &'a PostgresChannelRepository,
}

impl<'a> ChannelListHandler<'a> {
    pub fn new(channel_repository: &'a PostgresChannelRepository) -> Self {
        Self { channel_repository }
    }
}

impl<'a> ChannelListHandler<'a> {
    pub async fn handle(&self) -> Response<Body> {
        let channels = await!(self.channel_repository.list()).unwrap();
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