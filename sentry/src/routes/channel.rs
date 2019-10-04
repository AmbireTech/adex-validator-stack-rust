use crate::bad_request;
use hyper::{Body, Request, Response};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct ChannelListQuery {
    page: Option<u64>,
    validator: Option<String>,
}

pub fn handle_channel_routes(req: Request<Body>) -> Response<Body> {
    if req.uri().path().starts_with("/channel/list") {
        // @TODO: Get from Config
        let _channel_find_limit = 5;

        let query =
            serde_urlencoded::from_str::<ChannelListQuery>(&req.uri().query().unwrap_or(""));

        if query.is_err() {
            return bad_request();
        }

        println!("{:?}", query)
    }
    Response::new(Body::from("Channel!!"))
}
