use chrono::{serde::ts_milliseconds_option, DateTime, Utc};
use hyper::{Body, Request, Response};
use serde::Deserialize;

use primitives::{adapter::Adapter, sentry::EventAggregateResponse, Channel};

use crate::{db::list_event_aggregates, success_response, Application, Auth, ResponseError};

#[derive(Deserialize)]
pub struct EventAggregatesQuery {
    #[serde(default, with = "ts_milliseconds_option")]
    after: Option<DateTime<Utc>>,
}

pub async fn list_channel_event_aggregates<A: Adapter>(
    req: Request<Body>,
    app: &Application<A>,
) -> Result<Response<Body>, ResponseError> {
    let channel = req
        .extensions()
        .get::<Channel>()
        .expect("Request should have Channel");

    let auth = req
        .extensions()
        .get::<Auth>()
        .ok_or(ResponseError::Unauthorized)?;

    let query =
        serde_urlencoded::from_str::<EventAggregatesQuery>(req.uri().query().unwrap_or(""))?;

    let from = if channel.spec.validators.find(&auth.uid).is_some() {
        None
    } else {
        Some(auth.uid)
    };

    let event_aggregates = list_event_aggregates(
        &app.pool,
        &channel.id,
        app.config.events_find_limit,
        &from,
        &query.after,
    )
    .await?;

    let response = EventAggregateResponse {
        channel: channel.clone(),
        events: event_aggregates,
    };

    Ok(success_response(serde_json::to_string(&response)?))
}
