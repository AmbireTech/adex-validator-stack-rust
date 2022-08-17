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

use crate::{
    middleware::{
        auth::{
            authenticate_as_advertiser, authenticate_as_publisher, authentication_required,
            is_admin, AuthRequired, IsAdmin,
        },
        campaign::{called_by_creator, campaign_load, CalledByCreator, CampaignLoad},
        channel::{channel_load, ChannelLoad},
        Chain, Middleware,
    },
    response::ResponseError,
    routes::{
        analytics::analytics,
        campaign,
        campaign::{campaign_list, create_campaign, update_campaign},
        channel::{
            add_spender_leaf, channel_dummy_deposit, channel_list, channel_payout,
            get_accounting_for_channel, get_all_spender_limits, get_spender_limits, last_approved,
            validator_message::{
                create_validator_messages, extract_params, list_validator_messages,
            },
        },
    },
    Application, Auth,
};
use adapter::{prelude::*, Adapter, Dummy};
use axum::{
    middleware::{self, Next},
    routing::{get, post},
    Extension, Router,
};
use hyper::{Body, Method, Request, Response};
use once_cell::sync::Lazy;
use primitives::analytics::{
    query::{AllowedKey, ALLOWED_KEYS},
    AuthenticateAs,
};
use regex::Regex;
use tower::ServiceBuilder;

use super::{
    analytics::{analytics_axum, GET_ANALYTICS_ALLOWED_KEYS},
    channel::{
        channel_dummy_deposit_axum, channel_list_axum, channel_payout_axum,
        validator_message::{create_validator_messages_axum, list_validator_messages_axum},
    },
    units_for_slot::post_units_for_slot,
};

pub(crate) static LAST_APPROVED_BY_CHANNEL_ID: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^/v5/channel/0x([a-zA-Z0-9]{64})/last-approved/?$")
        .expect("The regex should be valid")
});

/// Only the initial Regex to be matched.
pub(crate) static CHANNEL_VALIDATOR_MESSAGES: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^/v5/channel/0x([a-zA-Z0-9]{64})/validator-messages(/.*)?$")
        .expect("The regex should be valid")
});

pub(crate) static CHANNEL_SPENDER_LEAF_AND_TOTAL_DEPOSITED: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^/v5/channel/0x([a-zA-Z0-9]{64})/spender/0x([a-zA-Z0-9]{40})/?$")
        .expect("This regex should be valid")
});

pub(crate) static INSERT_EVENTS_BY_CAMPAIGN_ID: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^/v5/campaign/(0x[a-zA-Z0-9]{32})/events/?$").expect("The regex should be valid")
});

pub(crate) static CLOSE_CAMPAIGN_BY_CAMPAIGN_ID: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^/v5/campaign/(0x[a-zA-Z0-9]{32})/close/?$").expect("The regex should be valid")
});

pub(crate) static CAMPAIGN_UPDATE_BY_ID: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^/v5/campaign/(0x[a-zA-Z0-9]{32})/?$").expect("The regex should be valid")
});

pub(crate) static CHANNEL_ALL_SPENDER_LIMITS: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^/v5/channel/0x([a-zA-Z0-9]{64})/spender/all/?$")
        .expect("The regex should be valid")
});

pub(crate) static CHANNEL_ACCOUNTING: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^/v5/channel/0x([a-zA-Z0-9]{64})/accounting/?$")
        .expect("The regex should be valid")
});

pub(crate) static CHANNEL_PAY: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^/v5/channel/0x([a-zA-Z0-9]{64})/pay/?$").expect("The regex should be valid")
});

/// When using [`adapter::Dummy`] you can set the Channel deposit for the Authenticated address.
pub(crate) static CHANNEL_DUMMY_ADAPTER_DEPOSIT: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^/v5/channel/dummy-deposit/?$").expect("The regex should be valid"));

/// Regex extracted parameters.
/// This struct is created manually on each of the matched routes.
#[derive(Debug, Clone)]
pub struct RouteParams(pub Vec<String>);

impl RouteParams {
    pub fn get(&self, index: usize) -> Option<String> {
        self.0.get(index).map(ToOwned::to_owned)
    }

    pub fn index(&self, i: usize) -> String {
        self.0[i].clone()
    }
}

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

pub fn channels_router_axum<C: Locked + 'static>() -> Router {
    let channel_routes = Router::new()
        .route(
            "/pay",
            post(channel_payout_axum::<C>)
                .route_layer(middleware::from_fn(authentication_required::<C, _>)),
        )
        .route(
            "/validator-messages",
            post(create_validator_messages_axum::<C>)
                .route_layer(middleware::from_fn(authentication_required::<C, _>)),
        )
        .route(
            "/validator-messages",
            get(list_validator_messages_axum::<C>),
        )
        // We allow Message Type filtering only when filtering by a ValidatorId
        .route(
            "/validator-messages/:address/*message_types",
            get(list_validator_messages_axum::<C>),
        )
        .layer(
            // keeps the order from top to bottom!
            ServiceBuilder::new()
                // Load the campaign from database based on the CampaignId
                .layer(middleware::from_fn(channel_load::<C, _>)),
        );

    Router::new()
        .route("/list", get(channel_list_axum::<C>))
        .nest("/:id", channel_routes)
        // Only available if Dummy Adapter is used!
        .route(
            "/dummy-deposit",
            post(channel_dummy_deposit_axum::<C>)
                .route_layer(middleware::from_fn(if_dummy_adapter::<C, _>))
                .route_layer(middleware::from_fn(authentication_required::<C, _>)),
        )
}

// TODO AIP#61: Add routes for:
// - GET /channel/:id/get-leaf
pub async fn channels_router<C: Locked + 'static>(
    mut req: Request<Body>,
    app: &Application<C>,
) -> Result<Response<Body>, ResponseError> {
    use std::any::Any;
    let (path, method) = (req.uri().path().to_owned(), req.method());

    // `GET /v5/channel/list`
    if let ("/v5/channel/list", &Method::GET) = (path.as_str(), method) {
        channel_list(req, app).await
    }
    // `GET /v5/channel/:id/last-approved`
    else if let (Some(caps), &Method::GET) = (LAST_APPROVED_BY_CHANNEL_ID.captures(&path), method)
    {
        let param = RouteParams(vec![caps
            .get(1)
            .map_or("".to_string(), |m| m.as_str().to_string())]);
        req.extensions_mut().insert(param);

        req = ChannelLoad.call(req, app).await?;

        last_approved(req, app).await
    }
    // `GET /v5/channel/:id/validator-messages`
    else if let (Some(caps), &Method::GET) = (CHANNEL_VALIDATOR_MESSAGES.captures(&path), method)
    {
        let param = RouteParams(vec![caps
            .get(1)
            .map_or("".to_string(), |m| m.as_str().to_string())]);

        req.extensions_mut().insert(param);

        req = ChannelLoad.call(req, app).await?;

        // @TODO: Move this to a middleware?!
        let extract_params = match extract_params(caps.get(2).map_or("", |m| m.as_str())) {
            Ok(params) => params,
            Err(error) => {
                return Err(error.into());
            }
        };

        list_validator_messages(req, app, &extract_params.0, &extract_params.1).await
    }
    // `POST /v5/channel/:id/validator-messages`
    else if let (Some(caps), &Method::POST) = (CHANNEL_VALIDATOR_MESSAGES.captures(&path), method)
    {
        let param = RouteParams(vec![caps
            .get(1)
            .map_or("".to_string(), |m| m.as_str().to_string())]);

        req.extensions_mut().insert(param);

        let req = Chain::new()
            .chain(AuthRequired)
            .chain(ChannelLoad)
            .apply(req, app)
            .await?;

        create_validator_messages(req, app).await
    }
    // `GET /v5/channel/:id/spender/:addr`
    else if let (Some(caps), &Method::GET) = (
        CHANNEL_SPENDER_LEAF_AND_TOTAL_DEPOSITED.captures(&path),
        method,
    ) {
        let param = RouteParams(vec![
            caps.get(1)
                .map_or("".to_string(), |m| m.as_str().to_string()), // channel ID
            caps.get(2)
                .map_or("".to_string(), |m| m.as_str().to_string()), // spender addr
        ]);
        req.extensions_mut().insert(param);
        req = Chain::new()
            .chain(AuthRequired)
            .chain(ChannelLoad)
            .apply(req, app)
            .await?;

        get_spender_limits(req, app).await
    }
    // `POST /v5/channel/:id/spender/:addr`
    else if let (Some(caps), &Method::POST) = (
        CHANNEL_SPENDER_LEAF_AND_TOTAL_DEPOSITED.captures(&path),
        method,
    ) {
        let param = RouteParams(vec![
            caps.get(1)
                .map_or("".to_string(), |m| m.as_str().to_string()), // channel ID
            caps.get(2)
                .map_or("".to_string(), |m| m.as_str().to_string()), // spender addr
        ]);
        req.extensions_mut().insert(param);
        req = Chain::new()
            .chain(AuthRequired)
            .chain(ChannelLoad)
            .apply(req, app)
            .await?;

        add_spender_leaf(req, app).await
    }
    // `GET /v5/channel/:id/spender/all`
    else if let (Some(caps), &Method::GET) = (CHANNEL_ALL_SPENDER_LIMITS.captures(&path), method)
    {
        let param = RouteParams(vec![caps
            .get(1)
            .map_or("".to_string(), |m| m.as_str().to_string())]);
        req.extensions_mut().insert(param);

        req = Chain::new()
            .chain(AuthRequired)
            .chain(ChannelLoad)
            .apply(req, app)
            .await?;

        get_all_spender_limits(req, app).await
    }
    // `GET /v5/channel/:id/accounting`
    else if let (Some(caps), &Method::GET) = (CHANNEL_ACCOUNTING.captures(&path), method) {
        let param = RouteParams(vec![caps
            .get(1)
            .map_or("".to_string(), |m| m.as_str().to_string())]);
        req.extensions_mut().insert(param);

        req = Chain::new()
            .chain(AuthRequired)
            .chain(ChannelLoad)
            .apply(req, app)
            .await?;

        get_accounting_for_channel(req, app).await
    }
    // POST /v5/channel/:id/pay
    else if let (Some(caps), &Method::POST) = (CHANNEL_PAY.captures(&path), method) {
        let param = RouteParams(vec![caps
            .get(1)
            .map_or("".to_string(), |m| m.as_str().to_string())]);
        req.extensions_mut().insert(param);

        req = Chain::new()
            .chain(AuthRequired)
            .chain(ChannelLoad)
            .apply(req, app)
            .await?;

        channel_payout(req, app).await
    }
    // POST /v5/channel/dummy-deposit
    // We allow the calling of the method only if we are using the Dummy adapter!
    else if let (Some(_caps), &Method::POST, true) = (
        CHANNEL_DUMMY_ADAPTER_DEPOSIT.captures(&path),
        method,
        <dyn Any + Send + Sync>::downcast_ref::<Adapter<Dummy>>(&app.adapter).is_some(),
    ) {
        req = Chain::new().chain(AuthRequired).apply(req, app).await?;

        channel_dummy_deposit(req, app).await
    } else {
        Err(ResponseError::NotFound)
    }
}

pub fn campaigns_router_axum<C: Locked + 'static>() -> Router {
    let campaign_routes = Router::new()
        .route(
            "/",
            // Campaign update
            post(campaign::update_campaign::handle_route_axum::<C>).route_layer(
                ServiceBuilder::new()
                    .layer(middleware::from_fn(authentication_required::<C, _>))
                    .layer(middleware::from_fn(called_by_creator::<C, _>)),
            ),
        )
        .route(
            "/events",
            post(campaign::insert_events::handle_route_axum::<C>),
        )
        .route(
            "/close",
            post(campaign::close_campaign_axum::<C>).route_layer(
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
        .route("/list", get(campaign::campaign_list_axum::<C>))
        .route(
            "/",
            // For creating campaigns
            post(campaign::create_campaign_axum::<C>),
        )
        .nest("/:id", campaign_routes)
}

/// `/v5/campaign` router
pub async fn campaigns_router<C: Locked + 'static>(
    mut req: Request<Body>,
    app: &Application<C>,
) -> Result<Response<Body>, ResponseError> {
    let (path, method) = (req.uri().path(), req.method());

    // For creating campaigns
    if (path, method) == ("/v5/campaign", &Method::POST) {
        let req = AuthRequired.call(req, app).await?;

        create_campaign(req, app).await
    } else if let (Some(_caps), &Method::POST) = (CAMPAIGN_UPDATE_BY_ID.captures(path), method) {
        let req = Chain::new()
            .chain(AuthRequired)
            .chain(CampaignLoad)
            .chain(CalledByCreator)
            .apply(req, app)
            .await?;

        update_campaign::handle_route(req, app).await
    } else if let (Some(caps), &Method::POST) =
        (INSERT_EVENTS_BY_CAMPAIGN_ID.captures(path), method)
    {
        let param = RouteParams(vec![caps
            .get(1)
            .map_or("".to_string(), |m| m.as_str().to_string())]);
        req.extensions_mut().insert(param);

        let req = CampaignLoad.call(req, app).await?;

        campaign::insert_events::handle_route(req, app).await
    } else if let (Some(caps), &Method::POST) =
        (CLOSE_CAMPAIGN_BY_CAMPAIGN_ID.captures(path), method)
    {
        let param = RouteParams(vec![caps
            .get(1)
            .map_or("".to_string(), |m| m.as_str().to_string())]);
        req.extensions_mut().insert(param);

        req = Chain::new()
            .chain(AuthRequired)
            .chain(CampaignLoad)
            .apply(req, app)
            .await?;

        campaign::close_campaign(req, app).await
    } else if method == Method::GET && path == "/v5/campaign/list" {
        campaign_list(req, app).await
    } else {
        Err(ResponseError::NotFound)
    }
}

pub async fn units_for_slot_router<C: Locked + 'static>(
    req: Request<Body>,
    app: &Application<C>,
) -> Result<Response<Body>, ResponseError> {
    let (route, method) = (req.uri().path(), req.method());

    match (method, route) {
        (&Method::POST, "/v5/units-for-slot") => post_units_for_slot(req, app).await,

        _ => Err(ResponseError::NotFound),
    }
}

/// `/v5/analytics` router
pub fn analytics_router_axum<C: Locked + 'static>() -> Router {
    let authenticated_analytics = Router::new()
        .route(
            "/for-advertiser",
            get(analytics_axum::<C>).route_layer(
                ServiceBuilder::new()
                    .layer(middleware::from_fn(authenticate_as_advertiser))
                    .layer(Extension(ALLOWED_KEYS.clone())),
            ),
        )
        .route(
            "/for-publisher",
            get(analytics_axum::<C>).route_layer(
                ServiceBuilder::new()
                    .layer(middleware::from_fn(authenticate_as_publisher))
                    .layer(Extension(ALLOWED_KEYS.clone())),
            ),
        )
        .route(
            "/for-admin",
            get(analytics_axum::<C>).route_layer(
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
            get(analytics_axum::<C>).route_layer(Extension(GET_ANALYTICS_ALLOWED_KEYS.clone())),
        )
        .merge(authenticated_analytics)
}

/// `/v5/analytics` router
pub async fn analytics_router<C: Locked + 'static>(
    mut req: Request<Body>,
    app: &Application<C>,
) -> Result<Response<Body>, ResponseError> {
    let (route, method) = (req.uri().path(), req.method());

    match (route, method) {
        ("/v5/analytics", &Method::GET) => {
            let allowed_keys_for_request = vec![AllowedKey::Country, AllowedKey::AdSlotType]
                .into_iter()
                .collect();
            analytics(req, app, Some(allowed_keys_for_request), None).await
        }
        ("/v5/analytics/for-advertiser", &Method::GET) => {
            let req = AuthRequired.call(req, app).await?;

            let authenticate_as = req
                .extensions()
                .get::<Auth>()
                .map(|auth| AuthenticateAs::Advertiser(auth.uid))
                .ok_or(ResponseError::Unauthorized)?;

            analytics(req, app, None, Some(authenticate_as)).await
        }
        ("/v5/analytics/for-publisher", &Method::GET) => {
            let authenticate_as = req
                .extensions()
                .get::<Auth>()
                .map(|auth| AuthenticateAs::Publisher(auth.uid))
                .ok_or(ResponseError::Unauthorized)?;

            let req = AuthRequired.call(req, app).await?;
            analytics(req, app, None, Some(authenticate_as)).await
        }
        ("/v5/analytics/for-admin", &Method::GET) => {
            req = Chain::new()
                .chain(AuthRequired)
                .chain(IsAdmin)
                .apply(req, app)
                .await?;
            analytics(req, app, None, None).await
        }
        _ => Err(ResponseError::NotFound),
    }
}
