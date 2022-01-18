use adapter::client::Locked;
use chrono::{serde::ts_milliseconds_option, DateTime, Utc};
use hyper::{Body, Request, Response};
use serde::Deserialize;

use primitives::{sentry::EventAggregateResponse, Channel};

use crate::{success_response, Application, Auth, ResponseError};

#[derive(Deserialize)]
pub struct EventAggregatesQuery {
    #[serde(default, with = "ts_milliseconds_option")]
    #[allow(dead_code)]
    after: Option<DateTime<Utc>>,
}
#[deprecated = "V5 - Double check what is need from the event aggregates from V4"]
pub async fn list_channel_event_aggregates<C: Locked + 'static>(
    req: Request<Body>,
    _app: &Application<C>,
) -> Result<Response<Body>, ResponseError> {
    let channel = *req
        .extensions()
        .get::<Channel>()
        .expect("Request should have Channel");

    let auth = req
        .extensions()
        .get::<Auth>()
        .ok_or(ResponseError::Unauthorized)?;

    let _query =
        serde_urlencoded::from_str::<EventAggregatesQuery>(req.uri().query().unwrap_or(""))?;

    let _from = channel.find_validator(auth.uid);

    let event_aggregates = vec![];
    // let event_aggregates = list_event_aggregates(
    //     &app.pool,
    //     &channel.id,
    //     app.config.events_find_limit,
    //     &from,
    //     &query.after,
    // )
    // .await?;

    let response = EventAggregateResponse {
        channel,
        events: event_aggregates,
    };

    Ok(success_response(serde_json::to_string(&response)?))
}
