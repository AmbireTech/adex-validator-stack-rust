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
        units_for_slot::get_units_for_slot,
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
        .route(
            "/validator-messages/:address",
            get(list_validator_messages::<C>),
        )
        // We allow Message Type filtering only when filtering by a ValidatorId
        .route(
            "/validator-messages/:address/:message_types",
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
    Router::new().route("/", get(get_units_for_slot::<C>))
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
    use crate::{
        db::{insert_channel, validator_message::insert_validator_message},
        test_util::{body_to, setup_dummy_app},
        Auth,
    };
    use adapter::ethereum::test_util::GANACHE_1;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use chrono::Utc;
    use primitives::{
        sentry::validator_messages::{MessageTypesFilter, ValidatorMessagesListResponse},
        test_util::{ADVERTISER, CAMPAIGNS, FOLLOWER, IDS, LEADER, PUBLISHER},
        validator::{Heartbeat, MessageType, MessageTypes, NewState},
    };
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
                    !app.config.sentry.admins.contains(&FOLLOWER),
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
                    app.config.sentry.admins.contains(&LEADER),
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

    #[tokio::test]
    async fn test_validator_messages_routes() -> Result<(), Box<dyn std::error::Error>> {
        let mut router = channels_router::<Dummy>();

        let app_guard = setup_dummy_app().await;
        let app = Arc::new(app_guard.app);

        let channel_context = Extension(CAMPAIGNS[0].clone().of_channel());

        insert_channel(&app.pool, &channel_context)
            .await
            .expect("should insert channel");

        let leader_messages = vec![
            MessageTypes::NewState(NewState {
                state_root: String::new(),
                signature: String::new(),
                balances: Default::default(),
            }),
            MessageTypes::Heartbeat(Heartbeat {
                signature: "of leader".into(),
                state_root: String::new(),
                timestamp: Utc::now(),
            }),
        ];
        let leader_auth = Auth {
            era: 1,
            uid: IDS[&LEADER],
            chain: channel_context.chain.clone(),
        };
        let follower_messages = vec![
            MessageTypes::NewState(NewState {
                state_root: String::new(),
                signature: String::new(),
                balances: Default::default(),
            }),
            MessageTypes::Heartbeat(Heartbeat {
                signature: "of follower".into(),
                state_root: String::new(),
                timestamp: Utc::now(),
            }),
        ];
        let follower_auth = Auth {
            era: 0,
            uid: IDS[&FOLLOWER],
            chain: channel_context.chain.clone(),
        };

        let all_messages = {
            let mut msgs = leader_messages.clone();
            msgs.extend(follower_messages.clone());

            msgs
        };

        // insert messages
        // for LEADER & FOLLOWER
        {
            for msg in leader_messages.iter() {
                assert!(
                    insert_validator_message(
                        &app.pool,
                        &channel_context.context,
                        &leader_auth.uid,
                        msg
                    )
                    .await?,
                    "Failed to insert leader message: {msg:?}"
                );
            }

            for msg in follower_messages.iter() {
                assert!(
                    insert_validator_message(
                        &app.pool,
                        &channel_context.context,
                        &follower_auth.uid,
                        msg
                    )
                    .await?,
                    "Failed to insert follower message: {msg:?}"
                );
            }
        }

        // GET /v5/channel/:id/validator-messages
        {
            let request = Request::builder()
                .uri(format!(
                    "/{id}/validator-messages",
                    id = channel_context.context.id()
                ))
                .extension(app.clone())
                .body(Body::empty())
                .unwrap();

            let response = router.call(request).await?;
            let status = response.status();

            assert_eq!(StatusCode::OK, status);
            let response = body_to::<ValidatorMessagesListResponse>(response).await?;

            for validator_message in response.messages.iter() {
                assert!(
                    all_messages.contains(&validator_message.msg),
                    "Inserted message not found in response {msg:?}",
                    msg = validator_message.msg
                );
            }
            assert_eq!(4, response.messages.len());
        }

        // GET /v5/channel/:id/validator-messages/:address
        // With LEADER
        {
            let request = Request::builder()
                .uri(format!(
                    "/{id}/validator-messages/{leader}",
                    id = channel_context.context.id(),
                    // Address & ValidatorId are displayed in the exact same way
                    leader = leader_auth.uid.to_address()
                ))
                .extension(app.clone())
                .body(Body::empty())
                .unwrap();

            let response = router.call(request).await?;
            let status = response.status();

            let response = body_to::<ValidatorMessagesListResponse>(response).await?;
            assert_eq!(StatusCode::OK, status);

            for validator_message in response.messages.iter() {
                assert!(
                    leader_messages.contains(&validator_message.msg),
                    "Inserted message not found in response {msg:?}",
                    msg = validator_message.msg
                );
            }

            assert_eq!(2, response.messages.len());
        }

        // GET /v5/channel/:id/validator-messages/:address/NewState
        // With FOLLOWER
        {
            let request = Request::builder()
                .uri(format!(
                    "/{id}/validator-messages/{follower}/{types}",
                    id = channel_context.context.id(),
                    // Address & ValidatorId are displayed in the exact same way
                    follower = follower_auth.uid.to_address(),
                    types = MessageTypesFilter(vec![MessageType::NewState])
                ))
                .extension(app.clone())
                .body(Body::empty())
                .unwrap();

            let response = router.call(request).await?;
            let status = response.status();

            let response = body_to::<ValidatorMessagesListResponse>(response).await?;
            assert_eq!(StatusCode::OK, status);

            assert_eq!(
                follower_messages[0].message_type(),
                MessageType::NewState,
                "You should not change the order of the FOLLOWER messages for this test"
            );
            assert_eq!(
                vec![follower_messages[0].clone()],
                response
                    .messages
                    .into_iter()
                    .map(|validator_message| validator_message.msg)
                    .collect::<Vec<_>>(),
                "Only a NewState message is expected"
            );
        }

        // GET /v5/channel/:id/validator-messages/:address/NewState+Heartbeat+RejectState
        // With LEADER
        {
            let request = Request::builder()
                .uri(format!(
                    "/{id}/validator-messages/{leader}/{types}",
                    id = channel_context.context.id(),
                    // Address & ValidatorId are displayed in the exact same way
                    leader = leader_auth.uid.to_address(),
                    types = MessageTypesFilter(vec![
                        MessageType::NewState,
                        MessageType::Heartbeat,
                        MessageType::RejectState
                    ])
                ))
                .extension(app.clone())
                .body(Body::empty())
                .unwrap();

            let response = router.call(request).await?;
            let status = response.status();

            let response = body_to::<ValidatorMessagesListResponse>(response).await?;
            assert_eq!(StatusCode::OK, status);

            for validator_message in response.messages.iter() {
                assert!(
                    leader_messages.contains(&validator_message.msg),
                    "Inserted message for leader not found in response {msg:?}",
                    msg = validator_message.msg
                );
            }

            assert_eq!(2, response.messages.len());
        }

        Ok(())
    }
}
