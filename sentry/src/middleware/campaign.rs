use std::sync::Arc;

use crate::{db::fetch_campaign, middleware::Middleware};
use crate::{response::ResponseError, routes::routers::RouteParams, Application, Auth};
use adapter::client::Locked;
use async_trait::async_trait;
use axum::{
    extract::{Path, RequestParts},
    middleware::Next,
};
use hyper::{Body, Request};
use primitives::{campaign::Campaign, CampaignId, ChainOf};

#[derive(Debug)]
pub struct CampaignLoad;
#[derive(Debug)]
pub struct CalledByCreator;

#[async_trait]
impl<C: Locked + 'static> Middleware<C> for CampaignLoad {
    async fn call<'a>(
        &self,
        mut request: Request<Body>,
        application: &'a Application<C>,
    ) -> Result<Request<Body>, ResponseError> {
        let id = request
            .extensions()
            .get::<RouteParams>()
            .ok_or_else(|| ResponseError::BadRequest("Route params not found".to_string()))?
            .get(0)
            .ok_or_else(|| ResponseError::BadRequest("No id".to_string()))?;

        let campaign_id = id
            .parse()
            .map_err(|_| ResponseError::BadRequest("Wrong Campaign Id".to_string()))?;
        let campaign = fetch_campaign(application.pool.clone(), &campaign_id)
            .await?
            .ok_or(ResponseError::NotFound)?;

        let campaign_context = application
            .config
            .find_chain_of(campaign.channel.token)
            .ok_or_else(|| ResponseError::BadRequest("Channel token not whitelisted".to_string()))?
            .with_campaign(campaign);

        // If this is an authenticated call
        // Check if the Campaign's Channel context (Chain Id) aligns with the Authentication token Chain id
        match request.extensions().get::<Auth>() {
            // If Chain Ids differ, the requester hasn't generated Auth token
            // to access the Channel in it's Chain Id.
            Some(auth) if auth.chain.chain_id != campaign_context.chain.chain_id => {
                return Err(ResponseError::Forbidden("Authentication token is generated for different Chain and differs from the Campaign's Channel Chain".into()))
            }
            _ => {},
        }

        request.extensions_mut().insert(campaign_context);

        Ok(request)
    }
}

pub async fn campaign_load<C: Locked + 'static, B>(
    request: axum::http::Request<B>,
    next: Next<B>,
) -> Result<axum::response::Response, ResponseError>
where
    B: Send,
{
    let (config, pool) = {
        let app = request
            .extensions()
            .get::<Arc<Application<C>>>()
            .expect("Application should always be present");

        (app.config.clone(), app.pool.clone())
    };

    // running extractors requires a `RequestParts`
    let mut request_parts = RequestParts::new(request);

    let campaign_id = request_parts
        .extract::<Path<CampaignId>>()
        .await
        .map_err(|_| ResponseError::BadRequest("Bad Campaign Id".to_string()))?;

    let campaign = fetch_campaign(pool.clone(), &campaign_id)
        .await?
        .ok_or(ResponseError::NotFound)?;

    let campaign_context = config
        .find_chain_of(campaign.channel.token)
        .ok_or_else(|| ResponseError::BadRequest("Channel token not whitelisted".to_string()))?
        .with_campaign(campaign);

    // If this is an authenticated call
    // Check if the Campaign's Channel context (Chain Id) aligns with the Authentication token Chain id
    match request_parts.extensions().get::<Auth>() {
            // If Chain Ids differ, the requester hasn't generated Auth token
            // to access the Channel in it's Chain Id.
            Some(auth) if auth.chain.chain_id != campaign_context.chain.chain_id => {
                return Err(ResponseError::Forbidden("Authentication token is generated for different Chain and differs from the Campaign's Channel Chain".into()))
            }
            _ => {},
        }

    request_parts.extensions_mut().insert(campaign_context);

    let request = request_parts.try_into_request().expect("Body extracted");

    Ok(next.run(request).await)
}

pub async fn called_by_creator<C: Locked + 'static, B>(
    request: axum::http::Request<B>,
    next: Next<B>,
) -> Result<axum::response::Response, ResponseError>
where
    B: Send,
{
    let campaign_context = request
        .extensions()
        .get::<ChainOf<Campaign>>()
        .expect("We must have a campaign in extensions");

    let auth = request
        .extensions()
        .get::<Auth>()
        .expect("request should have session");

    if auth.uid.to_address() != campaign_context.context.creator {
        return Err(ResponseError::Forbidden(
            "Request not sent by campaign creator".to_string(),
        ));
    }

    Ok(next.run(request).await)
}

#[async_trait]
impl<C: Locked + 'static> Middleware<C> for CalledByCreator {
    async fn call<'a>(
        &self,
        request: Request<Body>,
        _application: &'a Application<C>,
    ) -> Result<Request<Body>, ResponseError> {
        let campaign = request
            .extensions()
            .get::<Campaign>()
            .expect("We must have a campaign in extensions")
            .to_owned();

        let auth = request
            .extensions()
            .get::<Auth>()
            .expect("request should have session")
            .to_owned();

        if auth.uid.to_address() != campaign.creator {
            return Err(ResponseError::Forbidden(
                "Request not sent by campaign creator".to_string(),
            ));
        }

        Ok(request)
    }
}

#[cfg(test)]
mod test {
    use axum::{
        body::Body, http::StatusCode, middleware::from_fn, routing::get, Extension, Router,
    };
    use tower::Service;

    use adapter::Dummy;
    use primitives::{test_util::DUMMY_CAMPAIGN, Campaign, ChainOf};

    use crate::{
        db::{insert_campaign, insert_channel},
        test_util::setup_dummy_app,
    };

    use super::*;

    #[tokio::test]
    async fn test_campaign_loading() {
        let app_guard = setup_dummy_app().await;
        let app = Arc::new(app_guard.app);

        let build_request = |id: CampaignId| {
            Request::builder()
                .uri(format!("/{id}"))
                .extension(app.clone())
                .body(Body::empty())
                .expect("Should build Request")
        };

        let campaign = DUMMY_CAMPAIGN.clone();

        async fn handle(Extension(_campaign_context): Extension<ChainOf<Campaign>>) -> String {
            "Ok".into()
        }

        let mut router = Router::new()
            .route("/:id", get(handle))
            .layer(from_fn(campaign_load::<Dummy, _>));

        // bad CampaignId
        {
            let mut request = build_request(campaign.id);
            *request.uri_mut() = "/WrongCampaignId".parse().unwrap();

            let response = router
                .call(request)
                .await
                .expect("Should make request to Router");

            assert_eq!(
                StatusCode::BAD_REQUEST,
                // ResponseError::BadRequest("Wrong Campaign Id".to_string()),
                response.status()
            );
        }

        // non-existent campaign
        {
            let request = build_request(campaign.id);

            let response = router
                .call(request)
                .await
                .expect("Should make request to Router");

            assert_eq!(response.status(), StatusCode::NOT_FOUND);
        }

        // existing Campaign
        {
            let channel_chain = app
                .config
                .find_chain_of(DUMMY_CAMPAIGN.channel.token)
                .expect("Channel token should be whitelisted in config!");
            let channel_context = channel_chain.with_channel(DUMMY_CAMPAIGN.channel);
            // insert Channel
            insert_channel(&app.pool, &channel_context)
                .await
                .expect("Should insert Channel");
            // insert Campaign
            assert!(insert_campaign(&app.pool, &campaign)
                .await
                .expect("Should insert Campaign"));

            let request = build_request(campaign.id);

            let response = router
                .call(request)
                .await
                .expect("Should make request to Router");

            assert_eq!(response.status(), StatusCode::OK);
        }
    }
}
