//! This module contains all the Sentry REST API routers.
//!
//! Ideally these routers will be replaced with proper routing,
//! when we replace the custom `hyper` server setup with a web framework.
//!
//! # Routers
//!
//! Routers are functions that are called on certain route prefix (e.g. `/v5/channel`, `/v5/campaign`)
//! and they perform a few key operations for the REST API web server:
//!
//! - Extract parameters from the route
//! - Match against the different HTTP methods
//! - Calls additional [`middleware`](`crate::middleware`)s for the route
//!
use std::sync::Arc;

use axum::{
    http::Request,
    middleware::{self, Next},
    routing::{get, post},
    Extension, Router,
};
use tower::ServiceBuilder;

use adapter::{prelude::*, Adapter, Dummy};
use primitives::analytics::query::ALLOWED_KEYS;

use crate::{
    middleware::{
        auth::{
            authenticate_as_advertiser, authenticate_as_publisher, authentication_required,
            is_admin,
        },
        campaign::{called_by_creator, campaign_load},
        channel::channel_load,
    },
    routes::{
        analytics::{get_analytics, GET_ANALYTICS_ALLOWED_KEYS},
        campaign,
        channel::{
            add_spender_leaf, channel_dummy_deposit, channel_list, channel_payout,
            get_accounting_for_channel, get_all_spender_limits, get_leaf, get_spender_limits,
            last_approved,
            validator_message::{create_validator_messages, list_validator_messages},
        },
        units_for_slot::post_units_for_slot,
    },
    Application,
};

/// Middleware for Channel Dummy deposit
async fn if_dummy_adapter<C: Locked + 'static, B>(
    request: Request<B>,
    next: Next<B>,
) -> Result<axum::response::Response, axum::http::StatusCode> {
    use std::any::Any;

    let app = request
        .extensions()
        .get::<Arc<Application<C>>>()
        .expect("Application should always be present");

    if <dyn Any + Send + Sync>::downcast_ref::<Adapter<Dummy>>(&app.adapter).is_some() {
        Ok(next.run(request).await)
    } else {
        Err(axum::http::StatusCode::NOT_FOUND)
    }
}

#[derive(Clone)]
pub enum LeafFor {
    Earner,
    Spender,
}

/// `/v5/channel` router
pub fn channels_router<C: Locked + 'static>() -> Router {
    let spender_routes = Router::new()
        .route(
            "/:addr",
            get(get_spender_limits::<C>).post(add_spender_leaf::<C>),
        )
        .route("/all", get(get_all_spender_limits::<C>))
        .layer(
            // keeps the order from top to bottom!
            ServiceBuilder::new().layer(middleware::from_fn(authentication_required::<C, _>)),
        );

    let get_leaf_routes = Router::new()
        .route("/spender/:addr", get(get_leaf::<C>))
        .route_layer(Extension(LeafFor::Spender))
        .route("/earner/:addr", get(get_leaf::<C>))
        .route_layer(Extension(LeafFor::Earner));

    let channel_routes = Router::new()
        .route(
            "/pay",
            post(channel_payout::<C>)
                .route_layer(middleware::from_fn(authentication_required::<C, _>)),
        )
        .route("/accounting", get(get_accounting_for_channel::<C>))
        .route("/last-approved", get(last_approved::<C>))
        .nest("/spender", spender_routes)
        .nest("/get-leaf", get_leaf_routes)
        .route(
            "/validator-messages",
            post(create_validator_messages::<C>)
                .route_layer(middleware::from_fn(authentication_required::<C, _>)),
        )
        .route("/validator-messages", get(list_validator_messages::<C>))
        // We allow Message Type filtering only when filtering by a ValidatorId
        .route(
            "/validator-messages/:address/*message_types",
            get(list_validator_messages::<C>),
        )
        .layer(
            // keeps the order from top to bottom!
            ServiceBuilder::new()
                // Load the campaign from database based on the CampaignId
                .layer(middleware::from_fn(channel_load::<C, _>)),
        );

    Router::new()
        .route("/list", get(channel_list::<C>))
        .nest("/:id", channel_routes)
        // Only available if Dummy Adapter is used!
        .route(
            "/dummy-deposit",
            post(channel_dummy_deposit::<C>)
                .route_layer(middleware::from_fn(if_dummy_adapter::<C, _>))
                .route_layer(middleware::from_fn(authentication_required::<C, _>)),
        )
}

/// `/v5/campaign` router
pub fn campaigns_router<C: Locked + 'static>() -> Router {
    let campaign_routes = Router::new()
        .route(
            "/",
            // Campaign update
            post(campaign::update_campaign::handle_route::<C>).route_layer(
                ServiceBuilder::new()
                    .layer(middleware::from_fn(authentication_required::<C, _>))
                    .layer(middleware::from_fn(called_by_creator::<C, _>)),
            ),
        )
        .route("/events", post(campaign::insert_events::handle_route::<C>))
        .route(
            "/close",
            post(campaign::close_campaign::<C>).route_layer(
                ServiceBuilder::new()
                    .layer(middleware::from_fn(authentication_required::<C, _>))
                    .layer(middleware::from_fn(called_by_creator::<C, _>)),
            ),
        )
        .layer(
            // keeps the order from top to bottom!
            ServiceBuilder::new()
                // Load the campaign from database based on the CampaignId
                .layer(middleware::from_fn(campaign_load::<C, _>)),
        );

    Router::new()
        .route("/list", get(campaign::campaign_list::<C>))
        .route(
            "/",
            // For creating campaigns
            post(campaign::create_campaign::<C>),
        )
        .nest("/:id", campaign_routes)
}

/// `/v5/units-for-slot` router
pub fn units_for_slot_router<C: Locked + 'static>() -> Router {
    Router::new().route("/", post(post_units_for_slot::<C>))
}

/// `/v5/analytics` router
pub fn analytics_router<C: Locked + 'static>() -> Router {
    let authenticated_analytics = Router::new()
        .route(
            "/for-advertiser",
            get(get_analytics::<C>).route_layer(
                ServiceBuilder::new()
                    .layer(middleware::from_fn(authenticate_as_advertiser))
                    .layer(Extension(ALLOWED_KEYS.clone())),
            ),
        )
        .route(
            "/for-publisher",
            get(get_analytics::<C>).route_layer(
                ServiceBuilder::new()
                    .layer(middleware::from_fn(authenticate_as_publisher))
                    .layer(Extension(ALLOWED_KEYS.clone())),
            ),
        )
        .route(
            "/for-admin",
            get(get_analytics::<C>).route_layer(
                ServiceBuilder::new()
                    .layer(middleware::from_fn(is_admin::<C, _>))
                    .layer(Extension(ALLOWED_KEYS.clone())),
            ),
        )
        .layer(
            // keeps the order from top to bottom!
            ServiceBuilder::new()
                // authentication is required for all routes
                .layer(middleware::from_fn(authentication_required::<C, _>)),
        );

    Router::new()
        .route(
            "/",
            // only some keys are allowed for the default analytics route
            get(get_analytics::<C>).route_layer(Extension(GET_ANALYTICS_ALLOWED_KEYS.clone())),
        )
        .merge(authenticated_analytics)
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{test_util::setup_dummy_app, Auth};
    use adapter::ethereum::test_util::GANACHE_1;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use primitives::test_util::{ADVERTISER, FOLLOWER, IDS, LEADER, PUBLISHER};
    use tower::Service;

    #[tokio::test]
    async fn analytics_router_tests() {
        let mut router = analytics_router::<Dummy>();
        let app_guard = setup_dummy_app().await;
        let app = Arc::new(app_guard.app);

        // Test /for-publisher with no auth
        {
            let req = Request::builder()
                .uri("/for-publisher")
                .extension(app.clone())
                .body(Body::empty())
                .expect("Should build Request");

            let response = router
                .call(req)
                .await
                .expect("Should make request to Router");

            assert_eq!(StatusCode::UNAUTHORIZED, response.status());
        }
        // Test /for-publisher with auth
        {
            let req = Request::builder()
                .uri("/for-publisher")
                .extension(app.clone())
                .extension(Auth {
                    era: 1,
                    uid: IDS[&PUBLISHER],
                    chain: GANACHE_1.clone(),
                })
                .body(Body::empty())
                .expect("Should build Request");

            let response = router
                .call(req)
                .await
                .expect("Should make request to Router");

            assert_eq!(StatusCode::OK, response.status());
        }
        // Test /for-advertiser with no auth
        {
            let req = Request::builder()
                .uri("/for-advertiser")
                .extension(app.clone())
                .body(Body::empty())
                .expect("Should build Request");

            let response = router
                .call(req)
                .await
                .expect("Should make request to Router");

            assert_eq!(StatusCode::UNAUTHORIZED, response.status());
        }
        // Test /for-advertiser with auth
        {
            let req = Request::builder()
                .uri("/for-advertiser")
                .extension(app.clone())
                .extension(Auth {
                    era: 1,
                    uid: IDS[&ADVERTISER],
                    chain: GANACHE_1.clone(),
                })
                .body(Body::empty())
                .expect("Should build Request");

            let response = router
                .call(req)
                .await
                .expect("Should make request to Router");

            assert_eq!(StatusCode::OK, response.status());
        }
        // Test /for-admin with no auth
        {
            let req = Request::builder()
                .uri("/for-admin")
                .extension(app.clone())
                .body(Body::empty())
                .expect("Should build Request");

            let response = router
                .call(req)
                .await
                .expect("Should make request to Router");

            assert_eq!(StatusCode::UNAUTHORIZED, response.status());
        }
        // Test /for-admin with wrong auth
        {
            let not_admin = {
                assert!(
                    !app.config.admins.contains(&FOLLOWER),
                    "Should not contain the Follower as an Admin for this test!"
                );

                IDS[&FOLLOWER]
            };
            let req = Request::builder()
                .uri("/for-admin")
                .extension(app.clone())
                .extension(Auth {
                    era: 1,
                    uid: not_admin,
                    chain: GANACHE_1.clone(),
                })
                .body(Body::empty())
                .expect("Should build Request");

            let response = router
                .call(req)
                .await
                .expect("Should make request to Router");

            assert_eq!(StatusCode::UNAUTHORIZED, response.status());
        }
        // Test /for-admin with correct auth
        {
            let admin = {
                assert!(
                    app.config.admins.contains(&LEADER),
                    "Should contain the Leader as an Admin for this test!"
                );
                IDS[&LEADER]
            };
            let req = Request::builder()
                .uri("/for-admin")
                .extension(app.clone())
                .extension(Auth {
                    era: 1,
                    uid: admin,
                    chain: GANACHE_1.clone(),
                })
                .body(Body::empty())
                .expect("Should build Request");

            let response = router
                .call(req)
                .await
                .expect("Should make request to Router");

            assert_eq!(StatusCode::OK, response.status());
        }
    }
}
