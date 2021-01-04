use crate::db::event_aggregate::{latest_approve_state, latest_heartbeats, latest_new_state};
use crate::db::{
    get_channel_by_id, insert_channel, insert_validator_messages, list_channels,
    update_exhausted_channel,
};
use crate::{success_response, Application, Auth, ResponseError, RouteParams, Session};
use bb8::RunError;
use bb8_postgres::tokio_postgres::error;
use futures::future::try_join_all;
use hex::FromHex;
use hyper::{Body, Request, Response};
use primitives::{
    adapter::Adapter,
    sentry::{
        channel_list::{ChannelListQuery, LastApprovedQuery},
        Event, LastApproved, LastApprovedResponse, SuccessResponse,
    },
    validator::MessageTypes,
    Channel, ChannelId,
};
use slog::error;
use std::collections::HashMap;

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

    let channel = serde_json::from_slice::<Channel>(&body)
        .map_err(|e| ResponseError::FailedValidation(e.to_string()))?;

    if let Err(e) = app.adapter.validate_channel(&channel).await {
        return Err(ResponseError::BadRequest(e.to_string()));
    }

    let error_response = ResponseError::BadRequest("err occurred; please try again later".into());

    match insert_channel(&app.pool, &channel).await {
        Err(error) => {
            error!(&app.logger, "{}", &error; "module" => "create_channel");
            match error {
                RunError::User(e) if e.code() == Some(&error::SqlState::UNIQUE_VIOLATION) => Err(
                    ResponseError::Conflict("channel already exists".to_string()),
                ),
                _ => Err(error_response),
            }
        }
        Ok(false) => Err(error_response),
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

pub async fn channel_validate<A: Adapter>(
    req: Request<Body>,
    _: &Application<A>,
) -> Result<Response<Body>, ResponseError> {
    let body = hyper::body::to_bytes(req.into_body()).await?;
    let _channel = serde_json::from_slice::<Channel>(&body)
        .map_err(|e| ResponseError::FailedValidation(e.to_string()))?;
    let create_response = SuccessResponse { success: true };
    Ok(success_response(serde_json::to_string(&create_response)?))
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

    let default_response = Response::builder()
        .header("Content-type", "application/json")
        .body(
            serde_json::to_string(&LastApprovedResponse {
                last_approved: None,
                heartbeats: None,
            })?
            .into(),
        )
        .expect("should build response");

    let approve_state = match latest_approve_state(&app.pool, &channel).await? {
        Some(approve_state) => approve_state,
        None => return Ok(default_response),
    };

    let state_root = match approve_state.msg.clone() {
        MessageTypes::ApproveState(approve_state) => approve_state.state_root,
        _ => {
            error!(&app.logger, "{}", "failed to retrieve approved"; "module" => "last_approved");
            return Err(ResponseError::BadRequest("an error occurred".to_string()));
        }
    };

    let new_state = latest_new_state(&app.pool, &channel, &state_root).await?;
    if new_state.is_none() {
        return Ok(default_response);
    }

    let query = serde_urlencoded::from_str::<LastApprovedQuery>(&req.uri().query().unwrap_or(""))?;
    let validators = channel.spec.validators;
    let channel_id = channel.id;
    let heartbeats = if query.with_heartbeat.is_some() {
        let result = try_join_all(
            validators
                .iter()
                .map(|validator| latest_heartbeats(&app.pool, &channel_id, &validator.id)),
        )
        .await?;
        Some(result.into_iter().flatten().collect::<Vec<_>>())
    } else {
        None
    };

    Ok(Response::builder()
        .header("Content-type", "application/json")
        .body(
            serde_json::to_string(&LastApprovedResponse {
                last_approved: Some(LastApproved {
                    new_state,
                    approve_state: Some(approve_state),
                }),
                heartbeats,
            })?
            .into(),
        )
        .unwrap())
}

pub async fn insert_events<A: Adapter + 'static>(
    req: Request<Body>,
    app: &Application<A>,
) -> Result<Response<Body>, ResponseError> {
    let (req_head, req_body) = req.into_parts();

    let auth = req_head.extensions.get::<Auth>();
    let session = req_head
        .extensions
        .get::<Session>()
        .expect("request should have session");

    let route_params = req_head
        .extensions
        .get::<RouteParams>()
        .expect("request should have route params");

    let channel_id = ChannelId::from_hex(route_params.index(0))?;

    let body_bytes = hyper::body::to_bytes(req_body).await?;
    let request_body = serde_json::from_slice::<HashMap<String, Vec<Event>>>(&body_bytes)?;

    let events = request_body
        .get("events")
        .ok_or_else(|| ResponseError::BadRequest("invalid request".to_string()))?;

    app.event_aggregator
        .record(app, &channel_id, session, auth, &events)
        .await?;

    Ok(Response::builder()
        .header("Content-type", "application/json")
        .body(serde_json::to_string(&SuccessResponse { success: true })?.into())
        .unwrap())
}

pub async fn create_validator_messages<A: Adapter + 'static>(
    req: Request<Body>,
    app: &Application<A>,
) -> Result<Response<Body>, ResponseError> {
    let session = req
        .extensions()
        .get::<Auth>()
        .expect("auth request session")
        .to_owned();

    let channel = req
        .extensions()
        .get::<Channel>()
        .expect("Request should have Channel")
        .to_owned();

    let into_body = req.into_body();
    let body = hyper::body::to_bytes(into_body).await?;

    let request_body = serde_json::from_slice::<HashMap<String, Vec<MessageTypes>>>(&body)?;
    let messages = request_body
        .get("messages")
        .ok_or_else(|| ResponseError::BadRequest("missing messages body".to_string()))?;

    let channel_is_exhausted = messages.iter().any(|message| match message {
        MessageTypes::ApproveState(approve) => approve.exhausted,
        MessageTypes::NewState(new_state) => new_state.exhausted,
        _ => false,
    });

    match channel.spec.validators.find(&session.uid) {
        None => Err(ResponseError::Unauthorized),
        _ => {
            try_join_all(messages.iter().map(|message| {
                insert_validator_messages(&app.pool, &channel, &session.uid, &message)
            }))
            .await?;

            if channel_is_exhausted {
                if let Some(validator_index) = channel.spec.validators.find_index(&session.uid) {
                    update_exhausted_channel(&app.pool, &channel, validator_index).await?;
                }
            }

            Ok(success_response(serde_json::to_string(&SuccessResponse {
                success: true,
            })?))
        }
    }
}
