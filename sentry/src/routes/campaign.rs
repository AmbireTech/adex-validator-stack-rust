use std::collections::HashMap;

use crate::{success_response, Application, Auth, ResponseError, RouteParams, Session};
use hyper::{Body, Request, Response};
use primitives::{CampaignId, adapter::Adapter, sentry::{Event, SuccessResponse, campaign_create::CreateCampaign}};

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

    let route_params = req_head
        .extensions
        .get::<RouteParams>()
        .expect("request should have route params");

    let campaign_id: CampaignId = route_params.index(0).parse()?;

    let body_bytes = hyper::body::to_bytes(req_body).await?;
    let mut request_body = serde_json::from_slice::<HashMap<String, Vec<Event>>>(&body_bytes)?;

    let events = request_body
        .remove("events")
        .ok_or_else(|| ResponseError::BadRequest("invalid request".to_string()))?;

    //
    // TODO #381: AIP#61 Spender Aggregator should be called
    //

    // handle events - check access
    // handle events - Update targeting rules
    // calculate payout
    // distribute fees
    // handle spending - Spender Aggregate
    // handle events - aggregate Events and put into analytics

    Ok(Response::builder()
        .header("Content-type", "application/json")
        .body(serde_json::to_string(&SuccessResponse { success: true })?.into())
        .unwrap())
}