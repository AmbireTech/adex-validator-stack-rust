use std::collections::HashMap;

use crate::{
    access::{self, check_access},
    success_response, Application, Auth, ResponseError, Session,
};
use chrono::Utc;
use hyper::{Body, Request, Response};
use primitives::{
    adapter::Adapter,
    sentry::{campaign_create::CreateCampaign, Event, SuccessResponse},
    Campaign, CampaignId,
};
use redis::{aio::MultiplexedConnection, RedisError};

pub async fn create_campaign<A: Adapter>(
    req: Request<Body>,
    app: &Application<A>,
) -> Result<Response<Body>, ResponseError> {
    let body = hyper::body::to_bytes(req.into_body()).await?;

    let campaign = serde_json::from_slice::<CreateCampaign>(&body)
        .map_err(|e| ResponseError::FailedValidation(e.to_string()))?
        // create the actual `Campaign` with random `CampaignId`
        .into_campaign();

    // TODO AIP#61: Validate Campaign

    let error_response = ResponseError::BadRequest("err occurred; please try again later".into());

    // insert Campaign

    // match insert_campaign(&app.pool, &campaign).await {
    //     Err(error) => {
    //         error!(&app.logger, "{}", &error; "module" => "create_channel");

    //         match error {
    //             PoolError::Backend(error) if error.code() == Some(&SqlState::UNIQUE_VIOLATION) => {
    //                 Err(ResponseError::Conflict(
    //                     "channel already exists".to_string(),
    //                 ))
    //             }
    //             _ => Err(error_response),
    //         }
    //     }
    //     Ok(false) => Err(error_response),
    //     _ => Ok(()),
    // }?;

    let create_response = SuccessResponse { success: true };

    Ok(success_response(serde_json::to_string(&campaign)?))
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

    let campaign = req_head
        .extensions
        .get::<Campaign>()
        .expect("request should have a Campaign loaded");

    let body_bytes = hyper::body::to_bytes(req_body).await?;
    let mut request_body = serde_json::from_slice::<HashMap<String, Vec<Event>>>(&body_bytes)?;

    let events = request_body
        .remove("events")
        .ok_or_else(|| ResponseError::BadRequest("invalid request".to_string()))?;

    let processed = process_events(app, auth, session, campaign, events).await?;

    Ok(Response::builder()
        .header("Content-type", "application/json")
        .body(serde_json::to_string(&SuccessResponse { success: processed })?.into())
        .unwrap())
}

async fn process_events<A: Adapter + 'static>(
    app: &Application<A>,
    auth: Option<&Auth>,
    session: &Session,
    campaign: &Campaign,
    events: Vec<Event>,
) -> Result<bool, ResponseError> {
    if &Utc::now() > &campaign.active.to {
        return Err(ResponseError::BadRequest("Campaign is expired".into()));
    }

    //
    // TODO #381: AIP#61 Spender Aggregator should be called
    //

    // handle events - check access
    // handle events - Update targeting rules
    // calculate payout
    // distribute fees
    // handle spending - Spender Aggregate
    // handle events - aggregate Events and put into analytics

    check_access(
        &app.redis,
        session,
        auth,
        &app.config.ip_rate_limit,
        &campaign,
        &events,
    )
    .await
    .map_err(|e| match e {
        access::Error::ForbiddenReferrer => ResponseError::Forbidden(e.to_string()),
        access::Error::RulesError(error) => ResponseError::TooManyRequests(error),
        access::Error::UnAuthenticated => ResponseError::Unauthorized,
        _ => ResponseError::BadRequest(e.to_string()),
    })?;

    Ok(true)
}

pub async fn close_campaign<A: Adapter> (
    req: Request<Body>,
    app: &Application<A>,
) -> Result<Response<Body>, ResponseError> {
    // - only by creator
    // - sets redis remaining = 0 (`newBudget = totalSpent`, i.e. `newBudget = oldBudget - remaining`)
    let auth = req
        .extensions()
        .get::<Auth>();

    let campaign = req
        .extensions()
        .get::<Campaign>()
        .expect("We must have a campaign in extensions")
        .to_owned();

    let (is_creator, auth_uid) = match auth {
        Some(auth) => (auth.uid.to_address() == campaign.creator, auth.uid.to_string()),
        None => (false, Default::default()),
    };

    let has_close_event = true; // TODO: Discuss how get this info
    // Closing a campaign is allowed only by the creator
    if has_close_event && is_creator {
        set_campaign_remaining_to_zero(&app.redis, campaign.id).await.map_err(|_| ResponseError::BadRequest("couldn't close campaign".to_string()))?;
        return Ok(success_response(serde_json::to_string(&SuccessResponse {
            success: true,
        })?)); // TODO: Could there be a need to return the closed Campaign instead?
    }

    Err(ResponseError::Forbidden("Request not sent by campaign creator".to_string()))
}

// Not allowing to use SET with a predefined amount due to a possible race condition
// use increase/decrease functions instead
pub async fn set_campaign_remaining_to_zero(
    redis: &MultiplexedConnection,
    id: CampaignId
) -> Result<i64, RedisError> {
    let key = format!("{}:{}", "campaignRemaining", id); // TODO use key variable once 408 is merged
    redis::cmd("SET")
        .arg(&key)
        .arg(0)
        .query_async::<_, i64>(&mut redis.clone())
        .await
}