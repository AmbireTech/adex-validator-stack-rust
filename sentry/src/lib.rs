#![deny(clippy::all)]
#![deny(rust_2018_idioms)]
#![allow(deprecated)]
#![cfg_attr(docsrs, feature(doc_cfg))]

use adapter::{prelude::*, Adapter};
use chrono::Utc;
use hyper::{Body, Method, Request, Response, StatusCode};
use once_cell::sync::Lazy;
use primitives::{sentry::ValidationErrorResponse, Config, ValidatorId};
use redis::aio::MultiplexedConnection;
use regex::Regex;
use slog::Logger;
use std::collections::HashMap;
use {
    db::{CampaignRemaining, DbPool},
    middleware::{
        auth::{AuthRequired, Authenticate},
        campaign::{CalledByCreator, CampaignLoad},
        channel::ChannelLoad,
        cors::{cors, Cors},
        Chain, Middleware,
    },
    platform::PlatformApi,
    routes::{
        campaign,
        campaign::{campaign_list, create_campaign, update_campaign},
        channel::{
            add_spender_leaf, channel_list, channel_payout, get_accounting_for_channel,
            get_all_spender_limits, get_spender_limits, last_approved,
            validator_message::{
                create_validator_messages, extract_params, list_validator_messages,
            },
        },
        get_cfg,
        routers::analytics_router,
    },
};

pub mod access;
pub mod analytics;
pub mod application;
pub mod db;
pub mod middleware;
pub mod payout;
pub mod platform;
pub mod routes;
pub mod spender;

static LAST_APPROVED_BY_CHANNEL_ID: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^/v5/channel/0x([a-zA-Z0-9]{64})/last-approved/?$")
        .expect("The regex should be valid")
});

/// Only the initial Regex to be matched.
static CHANNEL_VALIDATOR_MESSAGES: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^/v5/channel/0x([a-zA-Z0-9]{64})/validator-messages(/.*)?$")
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
static CHANNEL_PAY: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^/v5/channel/0x([a-zA-Z0-9]{64})/pay/?$").expect("The regex should be valid")
});

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

/// The Sentry REST web application
pub struct Application<C: Locked + 'static> {
    /// For sentry to work properly, we need an [`adapter::Adapter`] in a [`adapter::LockedState`] state.
    pub adapter: Adapter<C>,
    pub config: Config,
    pub logger: Logger,
    pub redis: MultiplexedConnection,
    pub pool: DbPool,
    pub campaign_remaining: CampaignRemaining,
    pub platform_api: PlatformApi,
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
        platform_api: PlatformApi,
    ) -> Self {
        Self {
            adapter,
            config,
            logger,
            redis,
            pool,
            campaign_remaining,
            platform_api,
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
            ("/cfg", &Method::GET) => get_cfg(req, self).await,
            (route, _) if route.starts_with("/v5/analytics") => analytics_router(req, self).await,
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

// TODO AIP#61: Add routes for:
// - GET /channel/:id/get-leaf
async fn channels_router<C: Locked + 'static>(
    mut req: Request<Body>,
    app: &Application<C>,
) -> Result<Response<Body>, ResponseError> {
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

/// Sentry [`Application`] Session
#[derive(Debug, Clone)]
pub struct Session {
    pub ip: Option<String>,
    pub country: Option<String>,
    pub referrer_header: Option<String>,
    pub os: Option<String>,
}

/// Validated Authentication for the Sentry [`Application`].
#[derive(Debug, Clone)]
pub struct Auth {
    pub era: i64,
    pub uid: ValidatorId,
    /// The Chain for which this authentication was validated
    pub chain: primitives::Chain,
}

#[cfg(test)]
pub mod test_util {
    use adapter::{
        dummy::{Dummy, Options},
        Adapter,
    };
    use primitives::{
        config::GANACHE_CONFIG,
        test_util::{discard_logger, CREATOR, FOLLOWER, IDS, LEADER},
    };

    use crate::{
        db::{
            redis_pool::TESTS_POOL,
            tests_postgres::{setup_test_migrations, DATABASE_POOL},
            CampaignRemaining,
        },
        platform::PlatformApi,
        Application,
    };

    /// Uses development and therefore the goerli testnet addresses of the tokens
    /// It still uses DummyAdapter.
    pub async fn setup_dummy_app() -> Application<Dummy> {
        let config = GANACHE_CONFIG.clone();
        let adapter = Adapter::new(Dummy::init(Options {
            dummy_identity: IDS[&LEADER],
            dummy_auth_tokens: vec![
                (*CREATOR, "AUTH_Creator".into()),
                (*LEADER, "AUTH_Leader".into()),
                (*FOLLOWER, "AUTH_Follower".into()),
            ]
            .into_iter()
            .collect(),
        }));

        let redis = TESTS_POOL.get().await.expect("Should return Object");
        let database = DATABASE_POOL.get().await.expect("Should get a DB pool");

        setup_test_migrations(database.pool.clone())
            .await
            .expect("Migrations should succeed");

        let logger = discard_logger();

        let campaign_remaining = CampaignRemaining::new(redis.connection.clone());

        let platform_url = "http://change-me.tm".parse().expect("Bad ApiUrl!");
        let platform_api = PlatformApi::new(platform_url, config.platform.keep_alive_interval)
            .expect("should build test PlatformApi");

        let app = Application::new(
            adapter,
            config,
            logger,
            redis.connection.clone(),
            database.pool.clone(),
            campaign_remaining,
            platform_api,
        );

        app
    }
}
