use std::sync::Arc;

use axum::{
    extract::{Path, RequestParts},
    middleware::Next,
};
use serde::Deserialize;

use adapter::client::Locked;
use primitives::{campaign::Campaign, CampaignId, ChainOf};

use crate::{db::fetch_campaign, response::ResponseError, Application, Auth};

/// This struct is required because of routes that have more parameters
/// apart from the `CampaignId`
#[derive(Debug, Deserialize)]
struct CampaignParam {
    pub id: CampaignId,
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
        .extract::<Path<CampaignParam>>()
        .await
        .map_err(|_| ResponseError::BadRequest("Bad Campaign Id".to_string()))?
        .id;

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
        .expect("We must have a Campaign in extensions");

    let auth = request
        .extensions()
        .get::<Auth>()
        .expect("request should have an Authentication");

    if auth.uid.to_address() != campaign_context.context.creator {
        return Err(ResponseError::Forbidden(
            "Request not sent by Campaign's creator".to_string(),
        ));
    }

    Ok(next.run(request).await)
}

#[cfg(test)]
mod test {
    use axum::{
        body::Body,
        http::{Request, StatusCode},
        middleware::from_fn,
        routing::get,
        Extension, Router,
    };
    use tower::Service;

    use adapter::{ethereum::test_util::GANACHE_1, Dummy};
    use primitives::{
        test_util::{CAMPAIGNS, CREATOR, IDS, PUBLISHER},
        Campaign, ChainOf,
    };

    use crate::{
        db::{insert_campaign, insert_channel},
        test_util::setup_dummy_app,
    };

    use super::*;

    #[tokio::test]
    async fn test_campaign_loading() {
        let app_guard = setup_dummy_app().await;
        let app = Arc::new(app_guard.app);

        let campaign_context = CAMPAIGNS[0].clone();

        let build_request = || {
            Request::builder()
                .uri(format!("/{id}/test", id = campaign_context.context.id))
                .extension(app.clone())
                .body(Body::empty())
                .expect("Should build Request")
        };

        async fn handle(
            Extension(campaign_context): Extension<ChainOf<Campaign>>,
            Path((id, another)): Path<(CampaignId, String)>,
        ) -> String {
            assert_eq!(id, campaign_context.context.id);
            assert_eq!(another, "test");
            "Ok".into()
        }

        let mut router = Router::new()
            .route("/:id/:another", get(handle))
            .layer(from_fn(campaign_load::<Dummy, _>));

        // bad CampaignId
        {
            let mut request = build_request();
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

        // non-existent Campaign
        {
            let request = build_request();

            let response = router
                .call(request)
                .await
                .expect("Should make request to Router");

            assert_eq!(response.status(), StatusCode::NOT_FOUND);
        }

        // existing Campaign
        {
            let channel_context = campaign_context.of_channel();

            // insert Channel
            insert_channel(&app.pool, &channel_context)
                .await
                .expect("Should insert Channel");
            // insert Campaign
            assert!(insert_campaign(&app.pool, &campaign_context.context)
                .await
                .expect("Should insert Campaign"));

            let request = build_request();

            let response = router
                .call(request)
                .await
                .expect("Should make request to Router");

            assert_eq!(response.status(), StatusCode::OK);
        }
    }

    #[tokio::test]
    async fn test_called_by_creator() {
        let app_guard = setup_dummy_app().await;
        let app = Arc::new(app_guard.app);

        let campaign_context = CAMPAIGNS[0].clone();

        // insert Channel
        insert_channel(&app.pool, &campaign_context.of_channel())
            .await
            .expect("Should insert Channel");
        // insert Campaign
        assert!(insert_campaign(&app.pool, &campaign_context.context)
            .await
            .expect("Should insert Campaign"));

        let build_request = |auth: Auth| {
            Request::builder()
                .extension(app.clone())
                .extension(campaign_context.clone())
                .extension(auth)
                .body(Body::empty())
                .expect("Should build Request")
        };

        async fn handle(Extension(_campaign_context): Extension<ChainOf<Campaign>>) -> String {
            "Ok".into()
        }

        let mut router = Router::new()
            .route("/", get(handle))
            .layer(from_fn(called_by_creator::<Dummy, _>));

        // Not the Creator - Forbidden
        {
            let not_creator = Auth {
                era: 1,
                uid: IDS[&PUBLISHER],
                chain: campaign_context.chain.clone(),
            };
            assert_ne!(
                not_creator.uid.to_address(),
                campaign_context.context.creator,
                "The Auth address should not be the Campaign creator for this test!"
            );
            let request = build_request(not_creator);

            let response = router
                .call(request)
                .await
                .expect("Should make request to Router");

            assert_eq!(response.status(), StatusCode::FORBIDDEN);
        }

        // The Campaign Creator - Ok
        {
            let the_creator = Auth {
                era: 1,
                uid: IDS[&campaign_context.context.creator],
                chain: campaign_context.chain.clone(),
            };

            assert_eq!(
                the_creator.uid.to_address(),
                campaign_context.context.creator,
                "The Auth address should be the Campaign creator for this test!"
            );
            let request = build_request(the_creator);

            let response = router
                .call(request)
                .await
                .expect("Should make request to Router");

            assert_eq!(response.status(), StatusCode::OK);
        }
    }

    #[tokio::test]
    #[should_panic]
    async fn test_called_by_creator_no_auth() {
        let app_guard = setup_dummy_app().await;
        let app = Arc::new(app_guard.app);

        let campaign_context = CAMPAIGNS[0].clone();

        async fn handle(Extension(_campaign_context): Extension<ChainOf<Campaign>>) -> String {
            "Ok".into()
        }

        let mut router = Router::new()
            .route("/", get(handle))
            .layer(from_fn(called_by_creator::<Dummy, _>));

        // No Auth - Unauthorized
        let request = Request::builder()
            .extension(app.clone())
            .extension(campaign_context.clone())
            .body(Body::empty())
            .expect("Should build Request");

        let _response = router
            .call(request)
            .await
            .expect("Should make request to Router");
    }

    #[tokio::test]
    #[should_panic]
    async fn test_called_by_creator_no_campaign() {
        let app_guard = setup_dummy_app().await;
        let app = Arc::new(app_guard.app);

        let auth = Auth {
            era: 1,
            uid: IDS[&CREATOR],
            chain: GANACHE_1.clone(),
        };

        let request = Request::builder()
            .extension(app.clone())
            .extension(auth)
            .body(Body::empty())
            .expect("Should build Request");

        async fn handle(Extension(_campaign_context): Extension<ChainOf<Campaign>>) -> String {
            "Ok".into()
        }

        let mut router = Router::new()
            .route("/", get(handle))
            .layer(from_fn(called_by_creator::<Dummy, _>));

        let _response = router
            .call(request)
            .await
            .expect("Should make request to Router");
    }
}
