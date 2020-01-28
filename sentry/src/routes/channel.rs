use self::channel_list::ChannelListQuery;
use crate::db::{get_channel_by_id, insert_channel, list_channels, insert_validator_messages};
use crate::success_response;
use crate::Application;
use crate::ResponseError;
use crate::RouteParams;
use crate::Session;
use hex::FromHex;
use hyper::{Body, Request, Response};
use primitives::adapter::Adapter;
use primitives::sentry::{Event, SuccessResponse};
use primitives::{Channel, ChannelId};
use slog::error;
use std::collections::HashMap;
use primitives::channel::SpecValidator;
use primitives::validator::MessageTypes;
use futures::future::try_join_all;


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
    let body = hyper::body::to_bytes(req.into_body()).await?;

    let channel = serde_json::from_slice::<Channel>(&body)?;

    if let Err(e) = app.adapter.validate_channel(&channel).await {
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

pub async fn channel_list<A: Adapter>(
    req: Request<Body>,
    app: &Application<A>,
) -> Result<Response<Body>, ResponseError> {
    let query = serde_urlencoded::from_str::<ChannelListQuery>(&req.uri().query().unwrap_or(""))?;
    let skip = query
        .page
        .checked_mul(app.config.channels_find_limit.into())
        .ok_or_else(|| ResponseError::BadRequest("Page and/or limit is too large".into()))?;

    let list_response = list_channels(
        &app.pool,
        skip,
        app.config.channels_find_limit,
        &query.creator,
        &query.validator,
        &query.valid_until_ge,
    )
    .await?;

    Ok(success_response(serde_json::to_string(&list_response)?))
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

pub async fn insert_events<A: Adapter + 'static>(
    req: Request<Body>,
    app: &Application<A>,
) -> Result<Response<Body>, ResponseError> {
    let session = req
        .extensions()
        .get::<Session>()
        .expect("request session")
        .to_owned();

    let route_params = req
        .extensions()
        .get::<RouteParams>()
        .expect("request should have route params");

    let channel_id = ChannelId::from_hex(route_params.index(0))?;

    let into_body = req.into_body();
    let body = hyper::body::to_bytes(into_body).await?;
    let request_body = serde_json::from_slice::<HashMap<String, Vec<Event>>>(&body)?;
    let events = request_body
        .get("events")
        .ok_or_else(|| ResponseError::BadRequest("invalid request".to_string()))?;

    app.event_aggregator
        .record(app, &channel_id, &session, &events)
        .await?;

    Ok(Response::builder()
        .header("Content-type", "application/json")
        .body(serde_json::to_string(&SuccessResponse { success: true })?.into())
        .unwrap())
}

pub async fn validator_messages<A: Adapter + 'static>(
    req: Request<Body>,
    app: &Application<A>
) -> Result<Response<Body>, ResponseError> {
    let session = req
        .extensions()
        .get::<Session>()
        .expect("request session")
        .to_owned();

    let channel = req
        .extensions()
        .get::<Channel>()
        .expect("Request should have Channel")
        .to_owned();
    
    let into_body = req.into_body();
    let body = hyper::body::to_bytes(into_body).await?;
    let messages = serde_json::from_slice::<Vec<MessageTypes>>(&body)?;
    
    match channel.spec.validators.find(&session.uid) {
        SpecValidator::None => Err(ResponseError::Unauthorized),
        _  => {
            try_join_all(messages.iter().map(
                |message| insert_validator_messages(&app.pool, &channel, &session.uid, &message)
            )).await?;

            Ok(success_response(serde_json::to_string(&SuccessResponse { success: true })?))
        }
    }
    
}

mod channel_list {
    use chrono::serde::ts_seconds::deserialize as ts_seconds;
    use chrono::{DateTime, Utc};
    use primitives::ValidatorId;
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    pub(super) struct ChannelListQuery {
        #[serde(default = "default_page")]
        pub page: u64,
        /// filters the list on `valid_until >= valid_until_ge`
        /// It should be the same timestamp format as the `Channel.valid_until`: **seconds**
        #[serde(
            deserialize_with = "ts_seconds",
            default = "Utc::now",
            rename = "validUntil"
        )]
        pub valid_until_ge: DateTime<Utc>,
        pub creator: Option<String>,
        /// filters the channels containing a specific validator if provided
        pub validator: Option<ValidatorId>,
    }

    fn default_page() -> u64 {
        0
    }
}
