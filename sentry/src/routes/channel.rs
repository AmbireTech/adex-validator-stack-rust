use self::channel_list::ChannelListQuery;
use crate::db::{get_channel_by_id, insert_channel};
use crate::success_response;
use crate::Application;
use crate::ResponseError;
use crate::RouteParams;
use futures::TryStreamExt;
use hex::FromHex;
use hyper::{Body, Request, Response};
use primitives::adapter::Adapter;
use primitives::sentry::SuccessResponse;
use primitives::{Channel, ChannelId};
use slog::error;

pub async fn channel_status<A: Adapter>(
    req: Request<Body>,
    _: &Application<A>,
) -> Result<Response<Body>, ResponseError> {
    use serde::Serialize;
    #[derive(Serialize)]
    struct ChannelStatusResponse<'a> {
        channel: &'a Channel,
    }

    let channel = req
        .extensions()
        .get::<Channel>()
        .expect("Request should have Channel");

    let response = ChannelStatusResponse { channel };

    Ok(success_response(serde_json::to_string(&response)?))
}

pub async fn create_channel<A: Adapter>(
    req: Request<Body>,
    app: &Application<A>,
) -> Result<Response<Body>, ResponseError> {
    let body = req.into_body().try_concat().await?;

    let channel = serde_json::from_slice::<Channel>(&body)?;

    if let Err(e) = app.adapter.validate_channel(&channel) {
        return Err(ResponseError::BadRequest(e.to_string()));
    }

    match insert_channel(&app.pool, &channel).await {
        Err(err) => {
            error!(&app.logger, "{}", &err; "module" => "create_channel");
            Err(ResponseError::BadRequest(
                "err occurred; please try again later".into(),
            ))
        }
        Ok(false) => Err(ResponseError::BadRequest(
            "err occurred; please try again later".into(),
        )),
        _ => Ok(()),
    }?;

    let create_response = SuccessResponse { success: true };

    Ok(success_response(serde_json::to_string(&create_response)?))
}

pub async fn channel_list(req: Request<Body>) -> Result<Response<Body>, ResponseError> {
    // @TODO: Get from Config
    let _channel_find_limit = 5;

    let query = serde_urlencoded::from_str::<ChannelListQuery>(&req.uri().query().unwrap_or(""))?;

    // @TODO: List all channels returned from the DB
    println!("{:?}", query);

    Err(ResponseError::NotFound)
}

pub async fn last_approved<A: Adapter>(
    req: Request<Body>,
    app: &Application<A>,
) -> Result<Response<Body>, ResponseError> {
    // get request params
    let route_params = req
        .extensions()
        .get::<RouteParams>()
        .expect("request should have route params");
    let channel_id = ChannelId::from_hex(route_params.index(0))?;
    let channel = get_channel_by_id(&app.pool, &channel_id).await?.unwrap();

    Ok(Response::builder()
        .header("Content-type", "application/json")
        .body(serde_json::to_string(&channel)?.into())
        .unwrap())
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
