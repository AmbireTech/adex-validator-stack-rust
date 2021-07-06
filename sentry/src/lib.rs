#![deny(clippy::all)]
#![deny(rust_2018_idioms)]

use crate::db::{fetch_campaign, DbPool};
use crate::routes::campaign;
use crate::routes::channel::channel_status;
use crate::routes::event_aggregate::list_channel_event_aggregates;
use crate::routes::validator_message::{extract_params, list_validator_messages};
use chrono::Utc;
use hyper::{Body, Method, Request, Response, StatusCode};
use lazy_static::lazy_static;
use middleware::{
    auth::{AuthRequired, Authenticate},
    channel::{ChannelLoad, GetChannelId},
    cors::{cors, Cors},
};
use middleware::{campaign::CampaignLoad, Chain, Middleware};
use once_cell::sync::Lazy;
use primitives::adapter::Adapter;
use primitives::sentry::ValidationErrorResponse;
use primitives::{CampaignId, Config, ValidatorId};
use redis::aio::MultiplexedConnection;
use regex::Regex;
use routes::analytics::{advanced_analytics, advertiser_analytics, analytics, publisher_analytics};
use routes::campaign::{create_campaign, update_campaign};
use routes::cfg::config;
use routes::channel::{
    channel_list, channel_validate, create_channel, create_validator_messages, last_approved,
};
use slog::Logger;
use std::collections::HashMap;
use std::str::FromStr;

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
pub mod db;
// TODO AIP#61: remove the even aggregator once we've taken out the logic for AIP#61
// pub mod event_aggregator;
// TODO AIP#61: Remove even reducer or alter depending on our needs
// pub mod event_reducer;
pub mod payout;
pub mod spender;

lazy_static! {
    static ref CHANNEL_GET_BY_ID: Regex =
        Regex::new(r"^/channel/0x([a-zA-Z0-9]{64})/?$").expect("The regex should be valid");
    static ref LAST_APPROVED_BY_CHANNEL_ID: Regex = Regex::new(r"^/channel/0x([a-zA-Z0-9]{64})/last-approved/?$").expect("The regex should be valid");
    static ref CHANNEL_STATUS_BY_CHANNEL_ID: Regex = Regex::new(r"^/channel/0x([a-zA-Z0-9]{64})/status/?$").expect("The regex should be valid");
    // Only the initial Regex to be matched.
    static ref CHANNEL_VALIDATOR_MESSAGES: Regex = Regex::new(r"^/channel/0x([a-zA-Z0-9]{64})/validator-messages(/.*)?$").expect("The regex should be valid");
    static ref CHANNEL_EVENTS_AGGREGATES: Regex = Regex::new(r"^/channel/0x([a-zA-Z0-9]{64})/events-aggregates/?$").expect("The regex should be valid");
    static ref ANALYTICS_BY_CHANNEL_ID: Regex = Regex::new(r"^/analytics/0x([a-zA-Z0-9]{64})/?$").expect("The regex should be valid");
    static ref ADVERTISER_ANALYTICS_BY_CHANNEL_ID: Regex = Regex::new(r"^/analytics/for-advertiser/0x([a-zA-Z0-9]{64})/?$").expect("The regex should be valid");
    static ref PUBLISHER_ANALYTICS_BY_CHANNEL_ID: Regex = Regex::new(r"^/analytics/for-publisher/0x([a-zA-Z0-9]{64})/?$").expect("The regex should be valid");
    static ref CREATE_EVENTS_BY_CHANNEL_ID: Regex = Regex::new(r"^/channel/0x([a-zA-Z0-9]{64})/events/?$").expect("The regex should be valid");
}

static INSERT_EVENTS_BY_CAMPAIGN_ID: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^/campaign/0x([a-zA-Z0-9]{32})/events/?$").expect("The regex should be valid")
});
static CLOSE_CAMPAIGN_BY_CAMPAIGN_ID: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^/campaign/0x([a-zA-Z0-9]{32})/close/?$").expect("The regex should be valid")
});
static CAMPAIGN_UPDATE_BY_ID: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^/campaign/0x([a-zA-Z0-9]{32})/?$").expect("The regex should be valid")
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

#[derive(Clone)]
pub struct Application<A: Adapter> {
    pub adapter: A,
    pub logger: Logger,
    pub redis: MultiplexedConnection,
    pub pool: DbPool,
    pub config: Config,
}

impl<A: Adapter + 'static> Application<A> {
    pub fn new(
        adapter: A,
        config: Config,
        logger: Logger,
        redis: MultiplexedConnection,
        pool: DbPool,
    ) -> Self {
        Self {
            adapter,
            config,
            logger,
            redis,
            pool,
        }
    }

    pub async fn handle_routing(&self, req: Request<Body>) -> Response<Body> {
        let headers = match cors(&req) {
            Some(Cors::Simple(headers)) => headers,
            // if we have a Preflight, just return the response directly
            Some(Cors::Preflight(response)) => return response,
            None => Default::default(),
        };

        let req = match Authenticate.call(req, &self).await {
            Ok(req) => req,
            Err(error) => return map_response_error(error),
        };

        let mut response = match (req.uri().path(), req.method()) {
            ("/cfg", &Method::GET) => config(req, &self).await,
            ("/channel", &Method::POST) => create_channel(req, &self).await,
            ("/channel/list", &Method::GET) => channel_list(req, &self).await,
            ("/channel/validate", &Method::POST) => channel_validate(req, &self).await,

            ("/analytics", &Method::GET) => analytics(req, &self).await,
            ("/analytics/advanced", &Method::GET) => {
                let req = match AuthRequired.call(req, &self).await {
                    Ok(req) => req,
                    Err(error) => {
                        return map_response_error(error);
                    }
                };

                advanced_analytics(req, &self).await
            }
            ("/analytics/for-advertiser", &Method::GET) => {
                let req = match AuthRequired.call(req, &self).await {
                    Ok(req) => req,
                    Err(error) => {
                        return map_response_error(error);
                    }
                };
                advertiser_analytics(req, &self).await
            }
            ("/analytics/for-publisher", &Method::GET) => {
                let req = match AuthRequired.call(req, &self).await {
                    Ok(req) => req,
                    Err(error) => {
                        return map_response_error(error);
                    }
                };

                publisher_analytics(req, &self).await
            }
            // For creating campaigns
            ("/campaign", &Method::POST) => {
                let req = match AuthRequired.call(req, &self).await {
                    Ok(req) => req,
                    Err(error) => {
                        return map_response_error(error);
                    }
                };

                create_campaign(req, &self).await
            }
            (route, _) if route.starts_with("/analytics") => analytics_router(req, &self).await,
            // This is important because it prevents us from doing
            // expensive regex matching for routes without /channel
            (path, _) if path.starts_with("/channel") => channels_router(req, &self).await,
            (path, _) if path.starts_with("/campaign") => campaigns_router(req, &self).await,
            _ => Err(ResponseError::NotFound),
        }
        .unwrap_or_else(map_response_error);

        // extend the headers with the initial headers we have from CORS (if there are some)
        response.headers_mut().extend(headers);
        response
    }
}

async fn campaigns_router<A: Adapter + 'static>(
    mut req: Request<Body>,
    app: &Application<A>,
) -> Result<Response<Body>, ResponseError> {
    let (path, method) = (req.uri().path(), req.method());

    if let (Some(caps), &Method::POST) = (CAMPAIGN_UPDATE_BY_ID.captures(&path), method) {
        let param = RouteParams(vec![caps
            .get(1)
            .map_or("".to_string(), |m| m.as_str().to_string())]);

        // Should be safe to access indice here
        let campaign_id = param
            .get(0)
            .ok_or_else(|| ResponseError::BadRequest("No CampaignId".to_string()))?;
        let campaign_id = CampaignId::from_str(&campaign_id)
            .map_err(|_| ResponseError::BadRequest("Bad CampaignId".to_string()))?;

        let campaign = fetch_campaign(app.pool.clone(), &campaign_id)
            .await
            .map_err(|_| ResponseError::NotFound)?;

        req.extensions_mut().insert(campaign);
        update_campaign::handle_route(req, app).await
    } else if let (Some(caps), &Method::POST) =
        (INSERT_EVENTS_BY_CAMPAIGN_ID.captures(&path), method)
    {
        let param = RouteParams(vec![caps
            .get(1)
            .map_or("".to_string(), |m| m.as_str().to_string())]);
        req.extensions_mut().insert(param);

        let req = CampaignLoad.call(req, app).await?;

        campaign::insert_events(req, app).await
    } else if let (Some(_caps), &Method::POST) =
        (CLOSE_CAMPAIGN_BY_CAMPAIGN_ID.captures(&path), method)
    {
        // TODO AIP#61: Close campaign:
        // - only by creator
        // - sets redis remaining = 0 (`newBudget = totalSpent`, i.e. `newBudget = oldBudget - remaining`)

        // let (is_creator, auth_uid) = match auth {
        // Some(auth) => (auth.uid == channel.creator, auth.uid.to_string()),
        // None => (false, Default::default()),
        // };
        // Closing a campaign is allowed only by the creator
        // if has_close_event && is_creator {
        //     return Ok(());
        // }

        Err(ResponseError::NotFound)
    } else {
        Err(ResponseError::NotFound)
    }
}

async fn analytics_router<A: Adapter + 'static>(
    mut req: Request<Body>,
    app: &Application<A>,
) -> Result<Response<Body>, ResponseError> {
    let (route, method) = (req.uri().path(), req.method());

    // TODO AIP#61: Add routes for:
    // - POST /channel/:id/pay
    // #[serde(rename_all = "camelCase")]
    // Pay { payout: BalancesMap },
    //
    // - GET /channel/:id/spender/:addr
    // - GET /channel/:id/spender/all
    // - POST /channel/:id/spender/:addr
    // - GET /channel/:id/get-leaf
    match *method {
        Method::GET => {
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

async fn channels_router<A: Adapter + 'static>(
    mut req: Request<Body>,
    app: &Application<A>,
) -> Result<Response<Body>, ResponseError> {
    let (path, method) = (req.uri().path().to_owned(), req.method());

    // regex matching for routes with params
    /* if let (Some(caps), &Method::POST) = (CREATE_EVENTS_BY_CHANNEL_ID.captures(&path), method) {
        let param = RouteParams(vec![caps
            .get(1)
            .map_or("".to_string(), |m| m.as_str().to_string())]);

        req.extensions_mut().insert(param);

        insert_events(req, app).await
    } else */
    if let (Some(caps), &Method::GET) = (LAST_APPROVED_BY_CHANNEL_ID.captures(&path), method) {
        let param = RouteParams(vec![caps
            .get(1)
            .map_or("".to_string(), |m| m.as_str().to_string())]);
        req.extensions_mut().insert(param);

        last_approved(req, app).await
    } else if let (Some(caps), &Method::GET) =
        (CHANNEL_STATUS_BY_CHANNEL_ID.captures(&path), method)
    {
        let param = RouteParams(vec![caps
            .get(1)
            .map_or("".to_string(), |m| m.as_str().to_string())]);
        req.extensions_mut().insert(param);

        req = ChannelLoad.call(req, app).await?;
        channel_status(req, app).await
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

        list_validator_messages(req, &app, &extract_params.0, &extract_params.1).await
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

        create_validator_messages(req, &app).await
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

    let body = Body::from(serde_json::to_string(&error_response).expect("serialise err response"));

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

    let body = Body::from(serde_json::to_string(&error_response).expect("serialise err response"));

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
