#![deny(clippy::all)]
#![deny(rust_2018_idioms)]
#![allow(deprecated)]

use adapter::{prelude::*, Adapter};
use chrono::Utc;
use hyper::{Body, Method, Request, Response, StatusCode};
use middleware::{
    auth::{AuthRequired, Authenticate},
    campaign::{CalledByCreator, CampaignLoad},
    channel::{ChannelLoad, GetChannelId},
    cors::{cors, Cors},
    Chain, Middleware,
};
use once_cell::sync::Lazy;
use primitives::{sentry::ValidationErrorResponse, Config, ValidatorId};
use redis::aio::MultiplexedConnection;
use regex::Regex;
use slog::Logger;
use std::collections::HashMap;
use {
    db::{CampaignRemaining, DbPool},
    routes::{
        campaign,
        campaign::{campaign_list, create_campaign, update_campaign},
        cfg::config,
        channel::{
            channel_list, create_validator_messages, get_accounting_for_channel,
            get_all_spender_limits, get_spender_limits, last_approved,
        },
        event_aggregate::list_channel_event_aggregates,
        validator_message::{extract_params, list_validator_messages},
    },
};

pub mod analytics;
pub mod middleware;
pub mod routes {
    pub mod analytics;
    pub mod campaign;
    pub mod cfg;
    pub mod channel;
    pub mod event_aggregate;
    pub mod validator_message;
}

pub mod access;
pub mod analytics_recorder;
pub mod application;
pub mod db;
pub mod payout;
pub mod spender;

static LAST_APPROVED_BY_CHANNEL_ID: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^/v5/channel/0x([a-zA-Z0-9]{64})/last-approved/?$")
        .expect("The regex should be valid")
});
// Only the initial Regex to be matched.
static CHANNEL_VALIDATOR_MESSAGES: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^/v5/channel/0x([a-zA-Z0-9]{64})/validator-messages(/.*)?$")
        .expect("The regex should be valid")
});
static CHANNEL_EVENTS_AGGREGATES: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^/v5/channel/0x([a-zA-Z0-9]{64})/events-aggregates/?$")
        .expect("The regex should be valid")
});
static ANALYTICS_BY_CHANNEL_ID: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^/analytics/0x([a-zA-Z0-9]{64})/?$").expect("The regex should be valid")
});
static ADVERTISER_ANALYTICS_BY_CHANNEL_ID: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^/analytics/for-advertiser/0x([a-zA-Z0-9]{64})/?$")
        .expect("The regex should be valid")
});
static PUBLISHER_ANALYTICS_BY_CHANNEL_ID: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^/analytics/for-publisher/0x([a-zA-Z0-9]{64})/?$")
        .expect("The regex should be valid")
});
static CHANNEL_SPENDER_LEAF_AND_TOTAL_DEPOSITED: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^/v5/channel/0x([a-zA-Z0-9]{64})/spender/0x([a-zA-Z0-9]{40})/?$")
        .expect("This regex should be valid")
});

static INSERT_EVENTS_BY_CAMPAIGN_ID: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^/v5/campaign/(0x[a-zA-Z0-9]{32})/events/?$").expect("The regex should be valid")
});
static CLOSE_CAMPAIGN_BY_CAMPAIGN_ID: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^/v5/campaign/(0x[a-zA-Z0-9]{32})/close/?$").expect("The regex should be valid")
});
static CAMPAIGN_UPDATE_BY_ID: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^/v5/campaign/(0x[a-zA-Z0-9]{32})/?$").expect("The regex should be valid")
});
static CHANNEL_ALL_SPENDER_LIMITS: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^/v5/channel/0x([a-zA-Z0-9]{64})/spender/all/?$")
        .expect("The regex should be valid")
});
static CHANNEL_ACCOUNTING: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^/v5/channel/0x([a-zA-Z0-9]{64})/accounting/?$")
        .expect("The regex should be valid")
});

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

// #[derive(Clone)]
// pub struct Application<C: Locked + 'static> {
pub struct Application<C: Locked + 'static> {
    /// For sentry to work properly, we need an [`adapter::Adapter`] in a [`adapter::LockedState`] state.
    pub adapter: Adapter<C>,
    pub config: Config,
    pub logger: Logger,
    pub redis: MultiplexedConnection,
    pub pool: DbPool,
    pub campaign_remaining: CampaignRemaining,
}

impl<C> Application<C>
where
    C: Locked,
{
    pub fn new(
        adapter: Adapter<C>,
        config: Config,
        logger: Logger,
        redis: MultiplexedConnection,
        pool: DbPool,
        campaign_remaining: CampaignRemaining,
    ) -> Self {
        Self {
            adapter,
            config,
            logger,
            redis,
            pool,
            campaign_remaining,
        }
    }

    pub async fn handle_routing(&self, req: Request<Body>) -> Response<Body> {
        let headers = match cors(&req) {
            Some(Cors::Simple(headers)) => headers,
            // if we have a Preflight, just return the response directly
            Some(Cors::Preflight(response)) => return response,
            None => Default::default(),
        };

        let req = match Authenticate.call(req, self).await {
            Ok(req) => req,
            Err(error) => return map_response_error(error),
        };

        let mut response = match (req.uri().path(), req.method()) {
            ("/cfg", &Method::GET) => config(req, self).await,
            ("/channel/list", &Method::GET) => channel_list(req, self).await,
            (route, _) if route.starts_with("/analytics") => analytics_router(req, self).await,
            // This is important because it prevents us from doing
            // expensive regex matching for routes without /channel
            (path, _) if path.starts_with("/v5/channel") => channels_router(req, self).await,
            (path, _) if path.starts_with("/v5/campaign") => campaigns_router(req, self).await,
            _ => Err(ResponseError::NotFound),
        }
        .unwrap_or_else(map_response_error);

        // extend the headers with the initial headers we have from CORS (if there are some)
        response.headers_mut().extend(headers);
        response
    }
}

async fn campaigns_router<C: Locked + 'static>(
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

async fn analytics_router<C: Locked + 'static>(
    mut req: Request<Body>,
    app: &Application<C>,
) -> Result<Response<Body>, ResponseError> {
    use routes::analytics::{
        advanced_analytics, advertiser_analytics, analytics, publisher_analytics,
    };

    let (route, method) = (req.uri().path(), req.method());

    match (route, method) {
        ("/analytics", &Method::GET) => analytics(req, app).await,
        ("/analytics/advanced", &Method::GET) => {
            let req = AuthRequired.call(req, app).await?;

            advanced_analytics(req, app).await
        }
        ("/analytics/for-advertiser", &Method::GET) => {
            let req = AuthRequired.call(req, app).await?;
            advertiser_analytics(req, app).await
        }
        ("/analytics/for-publisher", &Method::GET) => {
            let req = AuthRequired.call(req, app).await?;

            publisher_analytics(req, app).await
        }
        (route, &Method::GET) => {
            if let Some(caps) = ANALYTICS_BY_CHANNEL_ID.captures(route) {
                let param = RouteParams(vec![caps
                    .get(1)
                    .map_or("".to_string(), |m| m.as_str().to_string())]);
                req.extensions_mut().insert(param);

                // apply middlewares
                req = Chain::new()
                    .chain(ChannelLoad)
                    .chain(GetChannelId)
                    .apply(req, app)
                    .await?;

                analytics(req, app).await
            } else if let Some(caps) = ADVERTISER_ANALYTICS_BY_CHANNEL_ID.captures(route) {
                let param = RouteParams(vec![caps
                    .get(1)
                    .map_or("".to_string(), |m| m.as_str().to_string())]);
                req.extensions_mut().insert(param);

                // apply middlewares
                req = Chain::new()
                    .chain(AuthRequired)
                    .chain(GetChannelId)
                    .apply(req, app)
                    .await?;

                advertiser_analytics(req, app).await
            } else if let Some(caps) = PUBLISHER_ANALYTICS_BY_CHANNEL_ID.captures(route) {
                let param = RouteParams(vec![caps
                    .get(1)
                    .map_or("".to_string(), |m| m.as_str().to_string())]);
                req.extensions_mut().insert(param);

                // apply middlewares
                req = Chain::new()
                    .chain(AuthRequired)
                    .chain(GetChannelId)
                    .apply(req, app)
                    .await?;

                publisher_analytics(req, app).await
            } else {
                Err(ResponseError::NotFound)
            }
        }
        _ => Err(ResponseError::NotFound),
    }
}

async fn channels_router<C: Locked + 'static>(
    mut req: Request<Body>,
    app: &Application<C>,
) -> Result<Response<Body>, ResponseError> {
    let (path, method) = (req.uri().path().to_owned(), req.method());

    // TODO AIP#61: Add routes for:
    // - POST /channel/:id/pay
    // #[serde(rename_all = "camelCase")]
    // Pay { payout: BalancesMap },
    //
    // - GET /channel/:id/spender/:addr
    // - GET /channel/:id/spender/all
    // - POST /channel/:id/spender/:addr
    // - GET /channel/:id/get-leaf
    if let (Some(caps), &Method::GET) = (LAST_APPROVED_BY_CHANNEL_ID.captures(&path), method) {
        let param = RouteParams(vec![caps
            .get(1)
            .map_or("".to_string(), |m| m.as_str().to_string())]);
        req.extensions_mut().insert(param);

        req = ChannelLoad.call(req, app).await?;

        last_approved(req, app).await
    } else if let (Some(caps), &Method::GET) = (CHANNEL_VALIDATOR_MESSAGES.captures(&path), method)
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
    } else if let (Some(caps), &Method::POST) = (CHANNEL_VALIDATOR_MESSAGES.captures(&path), method)
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
    } else if let (Some(caps), &Method::GET) = (CHANNEL_EVENTS_AGGREGATES.captures(&path), method) {
        req = AuthRequired.call(req, app).await?;

        let param = RouteParams(vec![
            caps.get(1)
                .map_or("".to_string(), |m| m.as_str().to_string()),
            caps.get(2)
                .map_or("".to_string(), |m| m.as_str().trim_matches('/').to_string()),
        ]);
        req.extensions_mut().insert(param);

        req = ChannelLoad.call(req, app).await?;

        list_channel_event_aggregates(req, app).await
    } else if let (Some(caps), &Method::GET) = (
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
    } else if let (Some(caps), &Method::GET) = (CHANNEL_ALL_SPENDER_LIMITS.captures(&path), method)
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
    } else if let (Some(caps), &Method::GET) = (CHANNEL_ACCOUNTING.captures(&path), method) {
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
    } else {
        Err(ResponseError::NotFound)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ResponseError {
    NotFound,
    BadRequest(String),
    FailedValidation(String),
    Unauthorized,
    Forbidden(String),
    Conflict(String),
    TooManyRequests(String),
}

impl<T> From<T> for ResponseError
where
    T: std::error::Error + 'static,
{
    fn from(error: T) -> Self {
        // @TODO use a error proper logger?
        println!("{:#?}", error);
        ResponseError::BadRequest("Bad Request: try again later".into())
    }
}
impl From<ResponseError> for Response<Body> {
    fn from(response_error: ResponseError) -> Self {
        map_response_error(response_error)
    }
}

pub fn map_response_error(error: ResponseError) -> Response<Body> {
    match error {
        ResponseError::NotFound => not_found(),
        ResponseError::BadRequest(e) => bad_response(e, StatusCode::BAD_REQUEST),
        ResponseError::Unauthorized => bad_response(
            "invalid authorization".to_string(),
            StatusCode::UNAUTHORIZED,
        ),
        ResponseError::Forbidden(e) => bad_response(e, StatusCode::FORBIDDEN),
        ResponseError::Conflict(e) => bad_response(e, StatusCode::CONFLICT),
        ResponseError::TooManyRequests(e) => bad_response(e, StatusCode::TOO_MANY_REQUESTS),
        ResponseError::FailedValidation(e) => bad_validation_response(e),
    }
}

pub fn not_found() -> Response<Body> {
    let mut response = Response::new(Body::from("Not found"));
    let status = response.status_mut();
    *status = StatusCode::NOT_FOUND;
    response
}

pub fn bad_response(response_body: String, status_code: StatusCode) -> Response<Body> {
    let mut error_response = HashMap::new();
    error_response.insert("message", response_body);

    let body = Body::from(serde_json::to_string(&error_response).expect("serialize err response"));

    let mut response = Response::new(body);
    response
        .headers_mut()
        .insert("Content-type", "application/json".parse().unwrap());

    *response.status_mut() = status_code;

    response
}

pub fn bad_validation_response(response_body: String) -> Response<Body> {
    let error_response = ValidationErrorResponse {
        status_code: 400,
        message: response_body.clone(),
        validation: vec![response_body],
    };

    let body = Body::from(serde_json::to_string(&error_response).expect("serialize err response"));

    let mut response = Response::new(body);
    response
        .headers_mut()
        .insert("Content-type", "application/json".parse().unwrap());

    *response.status_mut() = StatusCode::BAD_REQUEST;

    response
}

pub fn success_response(response_body: String) -> Response<Body> {
    let body = Body::from(response_body);

    let mut response = Response::new(body);
    response
        .headers_mut()
        .insert("Content-type", "application/json".parse().unwrap());

    let status = response.status_mut();
    *status = StatusCode::OK;

    response
}

pub fn epoch() -> f64 {
    Utc::now().timestamp() as f64 / 2_628_000_000.0
}

// @TODO: Make pub(crate)
#[derive(Debug, Clone)]
pub struct Session {
    pub ip: Option<String>,
    pub country: Option<String>,
    pub referrer_header: Option<String>,
    pub os: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Auth {
    pub era: i64,
    pub uid: ValidatorId,
}

#[cfg(test)]
pub mod test_util {
    use adapter::{
        dummy::{Dummy, Options},
        Adapter,
    };
    use primitives::{
        config::DEVELOPMENT_CONFIG,
        util::tests::{discard_logger, prep_db::IDS},
    };

    use crate::{
        db::{
            redis_pool::TESTS_POOL,
            tests_postgres::{setup_test_migrations, DATABASE_POOL},
            CampaignRemaining,
        },
        Application,
    };

    /// Uses development and therefore the goerli testnet addresses of the tokens
    /// It still uses DummyAdapter.
    pub async fn setup_dummy_app() -> Application<Dummy> {
        let config = DEVELOPMENT_CONFIG.clone();
        let adapter = Adapter::new(Dummy::init(
            Options {
                dummy_identity: IDS["leader"],
                dummy_auth: Default::default(),
                dummy_auth_tokens: Default::default(),
            },
            &config,
        ));

        let redis = TESTS_POOL.get().await.expect("Should return Object");
        let database = DATABASE_POOL.get().await.expect("Should get a DB pool");

        setup_test_migrations(database.pool.clone())
            .await
            .expect("Migrations should succeed");

        let campaign_remaining = CampaignRemaining::new(redis.connection.clone());

        let app = Application::new(
            adapter,
            config,
            discard_logger(),
            redis.connection.clone(),
            database.pool.clone(),
            campaign_remaining,
        );

        app
    }
}
