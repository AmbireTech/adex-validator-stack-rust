use futures::TryStreamExt;
use hyper::{Body, Method, Request, Response};

use primitives::adapter::Adapter;
use primitives::Channel;

use self::channel_list::ChannelListQuery;
use crate::ResponseError;

pub async fn handle_channel_routes(
    req: Request<Body>,
    adapter: impl Adapter,
) -> Result<Response<Body>, ResponseError> {
    // Channel Creates
    if req.uri().path() == "/channel" && req.method() == Method::POST {
        let body = req.into_body().try_concat().await?;
        let channel = serde_json::from_slice::<Channel>(&body)?;

        let create_response = channel_create::ChannelCreateResponse {
            success: adapter.validate_channel(&channel).unwrap_or(false),
        };
        let body = serde_json::to_string(&create_response)?.into();

        return Ok(Response::builder().status(200).body(body).unwrap());
    }

    // Channel List
    if req.uri().path().starts_with("/channel/list") {
        // @TODO: Get from Config
        let _channel_find_limit = 5;

        let query =
            serde_urlencoded::from_str::<ChannelListQuery>(&req.uri().query().unwrap_or(""))?;

        // @TODO: List all channels returned from the DB
        println!("{:?}", query);
    }

    Err(ResponseError::NotFound)
}

mod channel_create {
    use serde::Serialize;

    #[derive(Serialize)]
    pub(crate) struct ChannelCreateResponse {
        pub success: bool,
    }
}

mod channel_list {
    use chrono::{DateTime, Utc};
    use serde::{Deserialize, Deserializer};

    #[derive(Debug, Deserialize)]
    pub(crate) struct ChannelListQuery {
        /// page to show, should be >= 1
        #[serde(default = "default_page")]
        pub page: u64,
        /// channels limit per page, should be >= 1
        #[serde(default = "default_limit")]
        pub limit: u32,
        /// filters the list on `valid_until >= valid_until_ge`
        #[serde(default = "Utc::now")]
        pub valid_until_ge: DateTime<Utc>,
        /// filters the channels containing a specific validator if provided
        #[serde(default, deserialize_with = "deserialize_validator")]
        pub validator: Option<String>,
    }

    /// Deserialize the `Option<String>`, but if the `String` is empty it will return `None`
    fn deserialize_validator<'de, D>(de: D) -> Result<Option<String>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value: String = Deserialize::deserialize(de)?;
        let option = Some(value).filter(|string| !string.is_empty());
        Ok(option)
    }

    fn default_limit() -> u32 {
        1
    }

    fn default_page() -> u64 {
        1
    }
}
