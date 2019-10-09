use hyper::{Body, Method, Request, Response};
use serde::Deserialize;
use primitives::adapter::Adapter;
use primitives::Channel;
use futures::TryStreamExt;

#[derive(Debug, Deserialize)]
struct ChannelListQuery {
    page: Option<u64>,
    validator: Option<String>,
}

pub async fn handle_channel_routes(req: Request<Body>, adapter: impl Adapter) -> Result<Response<Body>, Box<dyn std::error::Error>> {
    // Channel Create
    if req.uri().path() == "/channel" && req.method() == Method::POST {
        let body = req.into_body().try_concat().await?;
        let channel = serde_json::from_slice::<Channel>(&body)?;

        return if adapter.validate_channel(&channel)? == true {
            Ok(Response::builder().status(200).body("OK".into()).unwrap())
        } else {
            Err("Channel is not valid".into())
        }
    }

    // Channel List
    if req.uri().path().starts_with("/channel/list") {
        // @TODO: Get from Config
        let _channel_find_limit = 5;

        let query =
            serde_urlencoded::from_str::<ChannelListQuery>(&req.uri().query().unwrap_or(""))?;

        // @TODO: List all channels returned from the DB
        println!("{:?}", query)
    }

    Err("Not found".into())
}
