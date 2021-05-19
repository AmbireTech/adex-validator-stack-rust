use crate::{success_response, Application, Auth, ResponseError, RouteParams, Session};
use hyper::{Body, Request, Response};
use primitives::{adapter::Adapter, sentry::{
    campaign_create::CreateCampaign,SuccessResponse}};

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
