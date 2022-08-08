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
use crate::{
    middleware::{
        auth::{AuthRequired, IsAdmin},
        campaign::{CalledByCreator, CampaignLoad},
        channel::ChannelLoad,
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
use hyper::{Body, Method, Request, Response};
use once_cell::sync::Lazy;
use primitives::analytics::{query::AllowedKey, AuthenticateAs};
use regex::Regex;

use super::units_for_slot::post_units_for_slot;

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
    else if let (Some(caps), &Method::POST, true) = (
        CHANNEL_DUMMY_ADAPTER_DEPOSIT.captures(&path),
        method,
        <dyn Any + Send + Sync>::downcast_ref::<Adapter<Dummy>>(&app.adapter).is_some(),
    ) {
        let param = RouteParams(vec![caps
            .get(1)
            .map_or("".to_string(), |m| m.as_str().to_string())]);
        req.extensions_mut().insert(param);

        req = Chain::new().chain(AuthRequired).apply(req, app).await?;

        channel_dummy_deposit(req, app).await
    } else {
        Err(ResponseError::NotFound)
    }
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

#[cfg(test)]
mod analytics_router_test {
    use crate::{
        db::{analytics::update_analytics, DbPool},
        response::ResponseError,
        test_util::setup_dummy_app,
        Auth,
    };

    use super::*;
    use adapter::ethereum::test_util::{GANACHE_1, GANACHE_1337};
    use chrono::Utc;
    use hyper::{Body, Request};
    use primitives::{
        analytics::{
            query::{AllowedKey, Time},
            AnalyticsQuery, Metric, OperatingSystem, Timeframe,
        },
        sentry::{
            AnalyticsResponse, DateHour, FetchedAnalytics, FetchedMetric, UpdateAnalytics, CLICK,
            IMPRESSION,
        },
        test_util::{ADVERTISER, DUMMY_CAMPAIGN, DUMMY_IPFS, IDS, LEADER, PUBLISHER, PUBLISHER_2},
        UnifiedNum,
    };

    async fn insert_mock_analytics(pool: &DbPool, base_datehour: DateHour<Utc>) {
        let analytics_base_hour = UpdateAnalytics {
            time: base_datehour,
            campaign_id: DUMMY_CAMPAIGN.id,
            ad_unit: DUMMY_IPFS[0],
            ad_slot: DUMMY_IPFS[1],
            ad_slot_type: None,
            advertiser: *ADVERTISER,
            publisher: *PUBLISHER,
            hostname: None,
            country: Some("Bulgaria".to_string()),
            os_name: OperatingSystem::map_os("Windows"),
            chain_id: GANACHE_1337.chain_id,
            event_type: CLICK,
            amount_to_add: UnifiedNum::from_u64(1_000_000),
            count_to_add: 1,
        };
        update_analytics(pool, analytics_base_hour)
            .await
            .expect("Should update analytics");

        let analytics_different_country = UpdateAnalytics {
            time: base_datehour,
            campaign_id: DUMMY_CAMPAIGN.id,
            ad_unit: DUMMY_IPFS[0],
            ad_slot: DUMMY_IPFS[1],
            ad_slot_type: None,
            advertiser: *ADVERTISER,
            publisher: *PUBLISHER,
            hostname: None,
            country: Some("Japan".to_string()),
            os_name: OperatingSystem::map_os("Windows"),
            chain_id: GANACHE_1337.chain_id,
            event_type: CLICK,
            amount_to_add: UnifiedNum::from_u64(1_000_000),
            count_to_add: 1,
        };
        update_analytics(pool, analytics_different_country)
            .await
            .expect("Should update analytics");

        let analytics_two_hours_ago = UpdateAnalytics {
            time: base_datehour - 2,
            campaign_id: DUMMY_CAMPAIGN.id,
            ad_unit: DUMMY_IPFS[0],
            ad_slot: DUMMY_IPFS[1],
            ad_slot_type: None,
            advertiser: *ADVERTISER,
            publisher: *PUBLISHER,
            hostname: None,
            country: Some("Bulgaria".to_string()),
            os_name: OperatingSystem::map_os("Windows"),
            chain_id: GANACHE_1337.chain_id,
            event_type: CLICK,
            amount_to_add: UnifiedNum::from_u64(1_000_000),
            count_to_add: 1,
        };
        update_analytics(pool, analytics_two_hours_ago)
            .await
            .expect("Should update analytics");

        let analytics_four_hours_ago = UpdateAnalytics {
            time: base_datehour - 4,
            campaign_id: DUMMY_CAMPAIGN.id,
            ad_unit: DUMMY_IPFS[0],
            ad_slot: DUMMY_IPFS[1],
            ad_slot_type: None,
            advertiser: *ADVERTISER,
            publisher: *PUBLISHER,
            hostname: None,
            country: Some("Bulgaria".to_string()),
            os_name: OperatingSystem::map_os("Windows"),
            chain_id: GANACHE_1337.chain_id,
            event_type: CLICK,
            amount_to_add: UnifiedNum::from_u64(1_000_000),
            count_to_add: 1,
        };
        update_analytics(pool, analytics_four_hours_ago)
            .await
            .expect("Should update analytics");

        let analytics_three_days_ago = UpdateAnalytics {
            time: base_datehour - (24 * 3),
            campaign_id: DUMMY_CAMPAIGN.id,
            ad_unit: DUMMY_IPFS[0],
            ad_slot: DUMMY_IPFS[1],
            ad_slot_type: None,
            advertiser: *ADVERTISER,
            publisher: *PUBLISHER,
            hostname: None,
            country: Some("Bulgaria".to_string()),
            os_name: OperatingSystem::map_os("Windows"),
            chain_id: GANACHE_1337.chain_id,
            event_type: CLICK,
            amount_to_add: UnifiedNum::from_u64(1_000_000),
            count_to_add: 1,
        };
        update_analytics(pool, analytics_three_days_ago)
            .await
            .expect("Should update analytics");
        // analytics from Base hour - 10 days ago
        let analytics_ten_days_ago = UpdateAnalytics {
            time: base_datehour - (24 * 10),
            campaign_id: DUMMY_CAMPAIGN.id,
            ad_unit: DUMMY_IPFS[0],
            ad_slot: DUMMY_IPFS[1],
            ad_slot_type: None,
            advertiser: *ADVERTISER,
            publisher: *PUBLISHER,
            hostname: None,
            country: Some("Bulgaria".to_string()),
            os_name: OperatingSystem::map_os("Windows"),
            chain_id: GANACHE_1337.chain_id,
            event_type: CLICK,
            amount_to_add: UnifiedNum::from_u64(1_000_000),
            count_to_add: 1,
        };
        update_analytics(pool, analytics_ten_days_ago)
            .await
            .expect("Should update analytics");

        let analytics_sixty_days_ago = UpdateAnalytics {
            time: base_datehour - (24 * 60),
            campaign_id: DUMMY_CAMPAIGN.id,
            ad_unit: DUMMY_IPFS[0],
            ad_slot: DUMMY_IPFS[1],
            ad_slot_type: None,
            advertiser: *ADVERTISER,
            publisher: *PUBLISHER,
            hostname: None,
            country: Some("Bulgaria".to_string()),
            os_name: OperatingSystem::map_os("Windows"),
            chain_id: GANACHE_1337.chain_id,
            event_type: CLICK,
            amount_to_add: UnifiedNum::from_u64(1_000_000),
            count_to_add: 1,
        };
        update_analytics(pool, analytics_sixty_days_ago)
            .await
            .expect("Should update analytics");

        let analytics_two_years_ago = UpdateAnalytics {
            time: base_datehour - (24 * 7 * 104),
            campaign_id: DUMMY_CAMPAIGN.id,
            ad_unit: DUMMY_IPFS[0],
            ad_slot: DUMMY_IPFS[1],
            ad_slot_type: None,
            advertiser: *ADVERTISER,
            publisher: *PUBLISHER,
            hostname: None,
            country: Some("Bulgaria".to_string()),
            os_name: OperatingSystem::map_os("Windows"),
            chain_id: GANACHE_1337.chain_id,
            event_type: CLICK,
            amount_to_add: UnifiedNum::from_u64(1_000_000),
            count_to_add: 1,
        };
        update_analytics(pool, analytics_two_years_ago)
            .await
            .expect("Should update analytics");

        let analytics_other_chain = UpdateAnalytics {
            time: base_datehour,
            campaign_id: DUMMY_CAMPAIGN.id,
            ad_unit: DUMMY_IPFS[0],
            ad_slot: DUMMY_IPFS[1],
            ad_slot_type: None,
            advertiser: *ADVERTISER,
            publisher: *PUBLISHER,
            hostname: None,
            country: Some("Bulgaria".to_string()),
            os_name: OperatingSystem::map_os("Windows"),
            chain_id: GANACHE_1.chain_id,
            event_type: CLICK,
            amount_to_add: UnifiedNum::from_u64(69_000_000),
            count_to_add: 69,
        };
        update_analytics(pool, analytics_other_chain)
            .await
            .expect("Should update analytics");
    }

    #[tokio::test]
    async fn test_analytics_route_for_guest() {
        let app = setup_dummy_app().await;
        // analytics for 17.01.2022 14:00:00
        // because we create mock Analytics for 72 hours ago and so on,
        // we need to fix the the base datehour which will ensure ensure that tests
        // that rely on relative hours & dates can be tested correctly.
        let base_datehour = DateHour::from_ymdh(2022, 1, 17, 14);
        insert_mock_analytics(&app.pool, base_datehour).await;

        // Test with empty query
        // Defaults for limit, event type, metric
        // Start date: now() - timeframe (which is a Day)
        //
        // limit: 100
        // eventType = IMPRESSION
        // metric = count
        // timeframe = day
        //
        // For this purpose we insert Analytics for Today & Yesterday
        // Since start date has default value & end date is optional
        {
            // start from 00:00:00 of today
            let today_midnight = DateHour::now().with_hour(0).expect("valid hour");
            let analytics_midnight = UpdateAnalytics {
                time: today_midnight,
                campaign_id: DUMMY_CAMPAIGN.id,
                ad_unit: DUMMY_IPFS[0],
                ad_slot: DUMMY_IPFS[1],
                ad_slot_type: None,
                advertiser: *ADVERTISER,
                publisher: *PUBLISHER,
                hostname: None,
                country: Some("Bulgaria".to_string()),
                os_name: OperatingSystem::map_os("Windows"),
                chain_id: GANACHE_1337.chain_id,
                event_type: IMPRESSION,
                amount_to_add: UnifiedNum::from_u64(25_000_000),
                count_to_add: 25,
            };
            update_analytics(&app.pool, analytics_midnight)
                .await
                .expect("Should update analytics");

            // more analytics for 01:00:00 of today
            let today_1am = DateHour::now().with_hour(1).expect("valid hour");
            let analytics_midnight = UpdateAnalytics {
                time: today_1am,
                campaign_id: DUMMY_CAMPAIGN.id,
                ad_unit: DUMMY_IPFS[0],
                ad_slot: DUMMY_IPFS[1],
                ad_slot_type: None,
                advertiser: *ADVERTISER,
                publisher: *PUBLISHER,
                hostname: None,
                country: Some("Bulgaria".to_string()),
                os_name: OperatingSystem::map_os("Windows"),
                chain_id: GANACHE_1337.chain_id,
                event_type: IMPRESSION,
                amount_to_add: UnifiedNum::from_u64(17_000_000),
                count_to_add: 17,
            };
            update_analytics(&app.pool, analytics_midnight)
                .await
                .expect("Should update analytics");

            // Yesterday 23:00 = Today at 23:00 - 24 hours
            let yesterday_23 = today_midnight.with_hour(23).expect("valid hour") - 24;
            let analytics_midnight = UpdateAnalytics {
                time: yesterday_23,
                campaign_id: DUMMY_CAMPAIGN.id,
                ad_unit: DUMMY_IPFS[0],
                ad_slot: DUMMY_IPFS[1],
                ad_slot_type: None,
                advertiser: *ADVERTISER,
                publisher: *PUBLISHER,
                hostname: None,
                country: Some("Bulgaria".to_string()),
                os_name: OperatingSystem::map_os("Windows"),
                chain_id: GANACHE_1337.chain_id,
                event_type: IMPRESSION,
                amount_to_add: UnifiedNum::from_u64(58_000_000),
                count_to_add: 58,
            };
            update_analytics(&app.pool, analytics_midnight)
                .await
                .expect("Should update analytics");

            let req = Request::builder()
                .uri("http://127.0.0.1/v5/analytics")
                .body(Body::empty())
                .expect("Should build Request");

            let analytics_response = analytics_router(req, &app)
                .await
                .expect("Should get analytics data");
            let json = hyper::body::to_bytes(analytics_response.into_body())
                .await
                .expect("Should get json");

            let fetched_analytics = serde_json::from_slice::<AnalyticsResponse>(&json)
                .expect("Should get analytics response")
                .analytics;

            assert_eq!(
                vec![
                    // yesterday at 23:00
                    FetchedMetric::Count(58),
                    // today at 00:00
                    FetchedMetric::Count(25),
                    // today at 01:00
                    FetchedMetric::Count(17),
                ],
                fetched_analytics
                    .iter()
                    .map(|fetched| fetched.value)
                    .collect::<Vec<_>>(),
                "Total of 4 count events for the 3 hours are expected"
            );
        }

        // Test with start date 1 hour ago
        // with base date hour
        // event type: CLICK
        {
            let query = AnalyticsQuery {
                limit: 1000,
                event_type: CLICK,
                metric: Metric::Count,
                segment_by: None,
                time: Time {
                    timeframe: Timeframe::Day,
                    start: base_datehour - 1,
                    end: None,
                },
                campaign_id: None,
                ad_unit: None,
                ad_slot: None,
                ad_slot_type: None,
                advertiser: None,
                publisher: None,
                hostname: None,
                country: None,
                os_name: None,
                chains: vec![GANACHE_1337.chain_id],
            };
            let query = serde_qs::to_string(&query).expect("should parse query");
            let req = Request::builder()
                .uri(format!("http://127.0.0.1/v5/analytics?{}", query))
                .body(Body::empty())
                .expect("Should build Request");

            let analytics_response = analytics_router(req, &app)
                .await
                .expect("Should get analytics data");
            let json = hyper::body::to_bytes(analytics_response.into_body())
                .await
                .expect("Should get json");

            let fetched_analytics = serde_json::from_slice::<AnalyticsResponse>(&json)
                .expect("Should get analytics response")
                .analytics;
            assert_eq!(
                vec![FetchedMetric::Count(2)],
                fetched_analytics
                    .iter()
                    .map(|analytics| analytics.value)
                    .collect::<Vec<_>>(),
            );
        }

        // Test with end date 1 hour ago
        {
            let query = AnalyticsQuery {
                limit: 1000,
                event_type: CLICK,
                metric: Metric::Count,
                segment_by: None,
                time: Time {
                    timeframe: Timeframe::Day,
                    start: base_datehour - &Timeframe::Day,
                    end: Some(base_datehour - 1),
                },
                campaign_id: None,
                ad_unit: None,
                ad_slot: None,
                ad_slot_type: None,
                advertiser: None,
                publisher: None,
                hostname: None,
                country: None,
                os_name: None,
                chains: vec![GANACHE_1337.chain_id],
            };
            let query = serde_qs::to_string(&query).expect("should parse query");
            let req = Request::builder()
                .uri(format!("http://127.0.0.1/v5/analytics?{}", query))
                .body(Body::empty())
                .expect("Should build Request");

            let analytics_response = analytics_router(req, &app)
                .await
                .expect("Should get analytics data");
            let json = hyper::body::to_bytes(analytics_response.into_body())
                .await
                .expect("Should get json");

            let fetched_analytics = serde_json::from_slice::<AnalyticsResponse>(&json)
                .expect("Should get analytics response")
                .analytics;

            assert_eq!(
                vec![FetchedMetric::Count(1), FetchedMetric::Count(1)],
                fetched_analytics
                    .iter()
                    .map(|analytics| analytics.value)
                    .collect::<Vec<_>>(),
            );
        }

        // Test with start_date and end_date
        // subtract 72 hours, there is an event exactly 72 hours ago so this also tests GTE
        {
            let query = AnalyticsQuery {
                limit: 1000,
                event_type: CLICK,
                metric: Metric::Count,
                segment_by: None,
                time: Time {
                    timeframe: Timeframe::Day,
                    // subtract 72 hours
                    start: base_datehour - 72,
                    // subtract 1 hour
                    end: Some(base_datehour - 1),
                },
                campaign_id: None,
                ad_unit: None,
                ad_slot: None,
                ad_slot_type: None,
                advertiser: None,
                publisher: None,
                hostname: None,
                country: None,
                os_name: None,
                chains: vec![GANACHE_1337.chain_id],
            };
            let query = serde_qs::to_string(&query).expect("should parse query");
            let req = Request::builder()
                .uri(format!("http://127.0.0.1/v5/analytics?{}", query))
                .body(Body::empty())
                .expect("Should build Request");
            let analytics_response = analytics_router(req, &app)
                .await
                .expect("Should get analytics data");
            let json = hyper::body::to_bytes(analytics_response.into_body())
                .await
                .expect("Should get json");

            let fetched_analytics = serde_json::from_slice::<AnalyticsResponse>(&json)
                .expect("Should get analytics response")
                .analytics;

            assert_eq!(
                vec![
                    FetchedMetric::Count(1),
                    FetchedMetric::Count(1),
                    FetchedMetric::Count(1)
                ],
                fetched_analytics
                    .iter()
                    .map(|fetched| fetched.value)
                    .collect::<Vec<_>>(),
                "We expect each analytics to have a count of 1"
            );
        }

        // Test with segment_by
        {
            let query = AnalyticsQuery {
                limit: 1000,
                event_type: CLICK,
                metric: Metric::Count,
                segment_by: Some(AllowedKey::Country),
                time: Time {
                    timeframe: Timeframe::Day,
                    start: base_datehour - &Timeframe::Day,
                    end: None,
                },
                campaign_id: None,
                ad_unit: None,
                ad_slot: None,
                ad_slot_type: None,
                advertiser: None,
                publisher: None,
                hostname: None,
                country: Some("Bulgaria".into()),
                os_name: None,
                chains: vec![GANACHE_1337.chain_id],
            };
            let query = serde_qs::to_string(&query).expect("should parse query");
            let req = Request::builder()
                .uri(format!("http://127.0.0.1/v5/analytics?{}", query))
                .body(Body::empty())
                .expect("Should build Request");

            let analytics_response = analytics_router(req, &app)
                .await
                .expect("Should get analytics data");
            let json = hyper::body::to_bytes(analytics_response.into_body())
                .await
                .expect("Should get json");

            let fetched_analytics = serde_json::from_slice::<AnalyticsResponse>(&json)
                .expect("Should get analytics response")
                .analytics;

            assert_eq!(
                vec![
                    FetchedMetric::Count(1),
                    FetchedMetric::Count(1),
                    FetchedMetric::Count(1)
                ],
                fetched_analytics
                    .iter()
                    .map(|fetched| fetched.value)
                    .collect::<Vec<_>>(),
                "We expect each analytics to have a count of 1"
            );
            assert!(
                fetched_analytics
                    .iter()
                    .all(|fetched| Some("Bulgaria".to_string()) == fetched.segment),
                "We expect each analytics to have segment Bulgaria"
            );
        }

        // Test with not allowed segment by
        // event type: IMPRESSION
        {
            let req = Request::builder()
            .uri("http://127.0.0.1/v5/analytics?limit=100&eventType=IMPRESSION&metric=count&timeframe=day&segmentBy=campaignId&campaignId=0x936da01f9abd4d9d80c702af85c822a8")
            .body(Body::empty())
            .expect("Should build Request");

            let analytics_response = analytics_router(req, &app)
                .await
                .expect_err("Should result in segmentBy error");

            assert_eq!(
                ResponseError::Forbidden("Disallowed segmentBy `campaignId`".into()),
                analytics_response,
            );
        }

        // test with not allowed key
        // event type: IMPRESSION
        {
            let req = Request::builder()
            .uri("http://127.0.0.1/v5/analytics?limit=100&eventType=CLICK&metric=count&timeframe=day&campaignId=0x936da01f9abd4d9d80c702af85c822a8")
            .body(Body::empty())
            .expect("Should build Request");

            let analytics_response = analytics_router(req, &app)
                .await
                .expect_err("Should get error for disallowed key");

            assert_eq!(
                ResponseError::Forbidden("Disallowed query key `campaignId`".into()),
                analytics_response,
            );
        }

        // test with different metric
        // with default start date
        // event type: IMPRESSION
        {
            let req = Request::builder()
            .uri("http://127.0.0.1/v5/analytics?limit=100&eventType=IMPRESSION&metric=paid&timeframe=day")
            .body(Body::empty())
            .expect("Should build Request");

            let analytics_response = analytics_router(req, &app)
                .await
                .expect("Should get analytics data");
            let json = hyper::body::to_bytes(analytics_response.into_body())
                .await
                .expect("Should get json");

            let fetched_analytics = serde_json::from_slice::<AnalyticsResponse>(&json)
                .expect("Should get analytics response")
                .analytics;

            assert_eq!(
                vec![
                    // yesterday at 23:00
                    FetchedMetric::Paid(58_000_000.into()),
                    // today at 00:00
                    FetchedMetric::Paid(25_000_000.into()),
                    // today at 01:00
                    FetchedMetric::Paid(17_000_000.into())
                ],
                fetched_analytics
                    .iter()
                    .map(|fetched| fetched.value)
                    .collect::<Vec<_>>(),
            );
        }

        // Test with different timeframe
        // with default start date
        // event type: IMPRESSION
        {
            let req = Request::builder()
            .uri("http://127.0.0.1/v5/analytics?limit=100&eventType=IMPRESSION&metric=count&timeframe=week")
            .body(Body::empty())
            .expect("Should build Request");

            let analytics_response = analytics_router(req, &app)
                .await
                .expect("Should get analytics data");
            let json = hyper::body::to_bytes(analytics_response.into_body())
                .await
                .expect("Should get json");

            let fetched_analytics = serde_json::from_slice::<AnalyticsResponse>(&json)
                .expect("Should get analytics response")
                .analytics;

            assert_eq!(
                vec![
                    // yesterday at 23:00
                    FetchedMetric::Count(58),
                    // today at 00:00
                    FetchedMetric::Count(25),
                    // today at 01:00
                    FetchedMetric::Count(17),
                ],
                fetched_analytics
                    .iter()
                    .map(|fetched| fetched.value)
                    .collect::<Vec<_>>(),
            );
        }

        // Test with a limit of 2
        // with default start date
        // event type: IMPRESSION
        {
            let req = Request::builder()
                .uri(
                    "http://127.0.0.1/v5/analytics?limit=2&eventType=IMPRESSION&metric=count&timeframe=day",
                )
                .body(Body::empty())
                .expect("Should build Request");

            let analytics_response = analytics_router(req, &app)
                .await
                .expect("Should get analytics data");
            let json = hyper::body::to_bytes(analytics_response.into_body())
                .await
                .expect("Should get json");

            let fetched_analytics = serde_json::from_slice::<AnalyticsResponse>(&json)
                .expect("Should get analytics response")
                .analytics;

            assert_eq!(
                // Limit is 2
                vec![
                    // yesterday at 23:00
                    FetchedMetric::Count(58),
                    // today at 00:00
                    FetchedMetric::Count(25),
                ],
                fetched_analytics
                    .iter()
                    .map(|fetched| fetched.value)
                    .collect::<Vec<_>>(),
            );
        }

        // Test with a month timeframe
        // with default start date
        // event type: IMPRESSION
        {
            let req = Request::builder()
            .uri(
                "http://127.0.0.1/v5/analytics?limit=100&eventType=IMPRESSION&metric=count&timeframe=month",
            )
            .body(Body::empty())
            .expect("Should build Request");

            let analytics_response = analytics_router(req, &app)
                .await
                .expect("Should get analytics data");
            let json = hyper::body::to_bytes(analytics_response.into_body())
                .await
                .expect("Should get json");

            let fetched_analytics = serde_json::from_slice::<AnalyticsResponse>(&json)
                .expect("Should get analytics response")
                .analytics;

            assert_eq!(
                vec![
                    // yesterday at 23:00
                    FetchedMetric::Count(58),
                    // 25 + 17
                    FetchedMetric::Count(42),
                ],
                fetched_analytics
                    .iter()
                    .map(|fetched| fetched.value)
                    .collect::<Vec<_>>(),
            );
        }

        // Test with a year timeframe
        // with default start date
        // event type: IMPRESSION
        {
            let req = Request::builder()
            .uri("http://127.0.0.1/v5/analytics?limit=100&eventType=IMPRESSION&metric=count&timeframe=year")
            .body(Body::empty())
            .expect("Should build Request");

            let analytics_response = analytics_router(req, &app)
                .await
                .expect("Should get analytics data");
            let json = hyper::body::to_bytes(analytics_response.into_body())
                .await
                .expect("Should get json");

            let fetched_analytics = serde_json::from_slice::<AnalyticsResponse>(&json)
                .expect("Should get analytics response")
                .analytics;

            assert_eq!(
                vec![FetchedMetric::Count(100)],
                fetched_analytics
                    .iter()
                    .map(|fetched| fetched.value)
                    .collect::<Vec<_>>(),
            );
        }

        // Test with start and end as timestamps
        // with Base date hour
        // event type: CLICK
        {
            let start_date = base_datehour - 72;
            // subtract 1 hour
            let end_date = base_datehour - 1;
            let query = AnalyticsQuery {
                limit: 1000,
                event_type: CLICK,
                metric: Metric::Count,
                segment_by: None,
                time: Time {
                    timeframe: Timeframe::Day,
                    start: start_date,
                    end: Some(end_date),
                },
                campaign_id: None,
                ad_unit: None,
                ad_slot: None,
                ad_slot_type: None,
                advertiser: None,
                publisher: None,
                hostname: None,
                country: None,
                os_name: None,
                chains: vec![],
            };
            let query = serde_qs::to_string(&query).expect("should parse query");
            let req = Request::builder()
                .uri(format!("http://127.0.0.1/v5/analytics?{}", query))
                .body(Body::empty())
                .expect("Should build Request");
            let analytics_response = analytics_router(req, &app)
                .await
                .expect("Should get analytics data");
            let json = hyper::body::to_bytes(analytics_response.into_body())
                .await
                .expect("Should get json");

            let fetched_analytics = serde_json::from_slice::<AnalyticsResponse>(&json)
                .expect("Should get analytics response")
                .analytics;

            assert_eq!(
                vec![
                    FetchedMetric::Count(1),
                    FetchedMetric::Count(1),
                    FetchedMetric::Count(1)
                ],
                fetched_analytics
                    .iter()
                    .map(|fetched| fetched.value)
                    .collect::<Vec<_>>(),
            );
        }

        // Test with a different chain
        // with base date hour
        // event type: CLICK
        {
            let query = AnalyticsQuery {
                limit: 1000,
                event_type: CLICK,
                metric: Metric::Count,
                segment_by: None,
                time: Time {
                    timeframe: Timeframe::Day,
                    start: base_datehour - 1,
                    end: None,
                },
                campaign_id: None,
                ad_unit: None,
                ad_slot: None,
                ad_slot_type: None,
                advertiser: None,
                publisher: None,
                hostname: None,
                country: None,
                os_name: None,
                chains: vec![GANACHE_1.chain_id],
            };
            let query = serde_qs::to_string(&query).expect("should parse query");
            let req = Request::builder()
                .uri(format!("http://127.0.0.1/v5/analytics?{}", query))
                .body(Body::empty())
                .expect("Should build Request");

            let analytics_response = analytics_router(req, &app)
                .await
                .expect("Should get analytics data");
            let json = hyper::body::to_bytes(analytics_response.into_body())
                .await
                .expect("Should get json");

            let fetched_analytics = serde_json::from_slice::<AnalyticsResponse>(&json)
                .expect("Should get analytics response")
                .analytics;
            assert_eq!(
                vec![FetchedMetric::Count(69)],
                fetched_analytics
                    .iter()
                    .map(|analytics| analytics.value)
                    .collect::<Vec<_>>(),
            );
        }

        // Test where we add more than one event count
        // with base date hour
        // event type: CLICK
        {
            let analytics_base_hour = UpdateAnalytics {
                time: base_datehour,
                campaign_id: DUMMY_CAMPAIGN.id,
                ad_unit: DUMMY_IPFS[0],
                ad_slot: DUMMY_IPFS[1],
                ad_slot_type: None,
                advertiser: *ADVERTISER,
                publisher: *PUBLISHER,
                hostname: None,
                country: Some("Bulgaria".to_string()),
                os_name: OperatingSystem::map_os("Windows"),
                chain_id: GANACHE_1337.chain_id,
                event_type: CLICK,
                amount_to_add: UnifiedNum::from_u64(1_000_000),
                count_to_add: 69,
            };
            update_analytics(&app.pool, analytics_base_hour)
                .await
                .expect("Should update analytics");

            let query = AnalyticsQuery {
                limit: 1000,
                event_type: CLICK,
                metric: Metric::Count,
                segment_by: None,
                time: Time {
                    timeframe: Timeframe::Day,
                    start: base_datehour - 1,
                    end: None,
                },
                campaign_id: None,
                ad_unit: None,
                ad_slot: None,
                ad_slot_type: None,
                advertiser: None,
                publisher: None,
                hostname: None,
                country: None,
                os_name: None,
                chains: vec![GANACHE_1337.chain_id],
            };
            let query = serde_qs::to_string(&query).expect("should parse query");
            let req = Request::builder()
                .uri(format!("http://127.0.0.1/v5/analytics?{}", query))
                .body(Body::empty())
                .expect("Should build Request");

            let analytics_response = analytics_router(req, &app)
                .await
                .expect("Should get analytics data");
            let json = hyper::body::to_bytes(analytics_response.into_body())
                .await
                .expect("Should get json");

            let fetched_analytics = serde_json::from_slice::<AnalyticsResponse>(&json)
                .expect("Should get analytics response")
                .analytics;

            // 1 + 1 (from previous events) + 69 (from update in this test case) = 71
            assert_eq!(
                vec![FetchedMetric::Count(71)],
                fetched_analytics
                    .iter()
                    .map(|analytics| analytics.value)
                    .collect::<Vec<_>>(),
            );
        }
        // Test with timeframe=day and start_date= 2 or more days ago to check if the results vec is split properly
    }

    async fn insert_mock_analytics_for_auth_routes(pool: &DbPool, base_datehour: DateHour<Utc>) {
        // Analytics with publisher and advertiser

        let analytics = UpdateAnalytics {
            time: base_datehour,
            campaign_id: DUMMY_CAMPAIGN.id,
            ad_unit: DUMMY_IPFS[0],
            ad_slot: DUMMY_IPFS[1],
            ad_slot_type: None,
            advertiser: *ADVERTISER,
            publisher: *PUBLISHER,
            hostname: None,
            country: Some("Bulgaria".to_string()),
            os_name: OperatingSystem::map_os("Windows"),
            chain_id: GANACHE_1337.chain_id,
            event_type: CLICK,
            amount_to_add: UnifiedNum::from_u64(1_000_000),
            count_to_add: 1,
        };
        update_analytics(pool, analytics)
            .await
            .expect("Should update analytics");
        // Analytics with a different unit/slot
        let analytics_different_slot_unit = UpdateAnalytics {
            time: base_datehour,
            campaign_id: DUMMY_CAMPAIGN.id,
            ad_unit: DUMMY_IPFS[2],
            ad_slot: DUMMY_IPFS[3],
            ad_slot_type: None,
            advertiser: *ADVERTISER,
            publisher: *PUBLISHER,
            hostname: None,
            country: Some("Bulgaria".to_string()),
            os_name: OperatingSystem::map_os("Windows"),
            chain_id: GANACHE_1337.chain_id,
            event_type: CLICK,
            amount_to_add: UnifiedNum::from_u64(1_000_000),
            count_to_add: 1,
        };
        update_analytics(pool, analytics_different_slot_unit)
            .await
            .expect("Should update analytics");
        // Analytics with a different event type
        let analytics_different_event = UpdateAnalytics {
            time: base_datehour,
            campaign_id: DUMMY_CAMPAIGN.id,
            ad_unit: DUMMY_IPFS[0],
            ad_slot: DUMMY_IPFS[1],
            ad_slot_type: None,
            advertiser: *ADVERTISER,
            publisher: *PUBLISHER,
            hostname: None,
            country: Some("Bulgaria".to_string()),
            os_name: OperatingSystem::map_os("Windows"),
            chain_id: GANACHE_1337.chain_id,
            event_type: IMPRESSION,
            amount_to_add: UnifiedNum::from_u64(1_000_000),
            count_to_add: 1,
        };
        update_analytics(pool, analytics_different_event)
            .await
            .expect("Should update analytics");
        // Analytics with no None fields
        let analytics_all_optional_fields = UpdateAnalytics {
            time: base_datehour - 2,
            campaign_id: DUMMY_CAMPAIGN.id,
            ad_unit: DUMMY_IPFS[0],
            ad_slot: DUMMY_IPFS[1],
            ad_slot_type: Some("TEST_TYPE".to_string()),
            advertiser: *ADVERTISER,
            publisher: *PUBLISHER,
            hostname: Some("localhost".to_string()),
            country: Some("Bulgaria".to_string()),
            os_name: OperatingSystem::map_os("Windows"),
            chain_id: GANACHE_1337.chain_id,
            event_type: CLICK,
            amount_to_add: UnifiedNum::from_u64(1_000_000),
            count_to_add: 1,
        };
        update_analytics(pool, analytics_all_optional_fields)
            .await
            .expect("Should update analytics");
        // Analytics with different publisher
        let analytics_different_publisher = UpdateAnalytics {
            time: base_datehour,
            campaign_id: DUMMY_CAMPAIGN.id,
            ad_unit: DUMMY_IPFS[0],
            ad_slot: DUMMY_IPFS[1],
            ad_slot_type: None,
            advertiser: *ADVERTISER,
            publisher: *PUBLISHER_2,
            hostname: None,
            country: Some("Bulgaria".to_string()),
            os_name: OperatingSystem::map_os("Windows"),
            chain_id: GANACHE_1337.chain_id,
            event_type: CLICK,
            amount_to_add: UnifiedNum::from_u64(1_000_000),
            count_to_add: 1,
        };
        update_analytics(pool, analytics_different_publisher)
            .await
            .expect("Should update analytics");
        // Analytics with different advertiser
        let analytics_different_advertiser = UpdateAnalytics {
            time: base_datehour,
            campaign_id: DUMMY_CAMPAIGN.id,
            ad_unit: DUMMY_IPFS[0],
            ad_slot: DUMMY_IPFS[1],
            ad_slot_type: None,
            advertiser: *ADVERTISER,
            publisher: *PUBLISHER,
            hostname: None,
            country: Some("Bulgaria".to_string()),
            os_name: OperatingSystem::map_os("Windows"),
            chain_id: GANACHE_1337.chain_id,
            event_type: CLICK,
            amount_to_add: UnifiedNum::from_u64(1_000_000),
            count_to_add: 1,
        };
        update_analytics(pool, analytics_different_advertiser)
            .await
            .expect("Should update analytics");
        // Analytics with both a different publisher and advertiser
        let analytics_different_publisher_advertiser = UpdateAnalytics {
            time: base_datehour,
            campaign_id: DUMMY_CAMPAIGN.id,
            ad_unit: DUMMY_IPFS[0],
            ad_slot: DUMMY_IPFS[1],
            ad_slot_type: None,
            advertiser: *ADVERTISER,
            publisher: *PUBLISHER_2,
            hostname: None,
            country: Some("Bulgaria".to_string()),
            os_name: OperatingSystem::map_os("Windows"),
            chain_id: GANACHE_1337.chain_id,
            event_type: CLICK,
            amount_to_add: UnifiedNum::from_u64(1_000_000),
            count_to_add: 1,
        };
        update_analytics(pool, analytics_different_publisher_advertiser)
            .await
            .expect("Should update analytics");
        let analytics_different_chain = UpdateAnalytics {
            time: base_datehour,
            campaign_id: DUMMY_CAMPAIGN.id,
            ad_unit: DUMMY_IPFS[0],
            ad_slot: DUMMY_IPFS[1],
            ad_slot_type: None,
            advertiser: *ADVERTISER,
            publisher: *PUBLISHER,
            hostname: None,
            country: Some("Bulgaria".to_string()),
            os_name: OperatingSystem::map_os("Windows"),
            chain_id: GANACHE_1.chain_id,
            event_type: CLICK,
            amount_to_add: UnifiedNum::from_u64(69_000_000),
            count_to_add: 69,
        };
        update_analytics(pool, analytics_different_chain)
            .await
            .expect("Should update analytics");
    }

    #[tokio::test]
    async fn test_analytics_router_with_auth() {
        let app = setup_dummy_app().await;
        // 27.12.2021 23:00:00
        let base_datehour = DateHour::from_ymdh(2021, 12, 27, 23);
        let query = AnalyticsQuery {
            limit: 100,
            event_type: CLICK,
            metric: Metric::Count,
            segment_by: Some(AllowedKey::Country),
            time: Time {
                timeframe: Timeframe::Day,
                // Midnight of base datehour
                start: base_datehour.with_hour(0).expect("Correct hour"),
                end: None,
            },
            campaign_id: None,
            ad_unit: None,
            ad_slot: None,
            ad_slot_type: None,
            advertiser: None,
            publisher: None,
            hostname: None,
            country: None,
            os_name: None,
            chains: vec![],
        };
        let base_query = serde_qs::to_string(&query).expect("should parse query");

        insert_mock_analytics_for_auth_routes(&app.pool, base_datehour).await;

        let publisher_auth = Auth {
            era: 0,
            uid: IDS[&PUBLISHER],
            chain: GANACHE_1337.clone(),
        };
        let advertiser_auth = Auth {
            era: 0,
            uid: IDS[&ADVERTISER],
            chain: GANACHE_1337.clone(),
        };
        let admin_auth = Auth {
            era: 0,
            uid: IDS[&LEADER],
            chain: GANACHE_1337.clone(),
        };
        let admin_auth_other_chain = Auth {
            era: 0,
            uid: IDS[&LEADER],
            chain: GANACHE_1.clone(),
        };

        // test for publisher
        {
            let req = Request::builder()
                .extension(publisher_auth.clone())
                .uri(format!(
                    "http://127.0.0.1/v5/analytics/for-publisher?{}",
                    base_query
                ))
                .body(Body::empty())
                .expect("Should build Request");

            let analytics_response = analytics_router(req, &app)
                .await
                .expect("Should get analytics data");
            let json = hyper::body::to_bytes(analytics_response.into_body())
                .await
                .expect("Should get json");

            let fetched_analytics = serde_json::from_slice::<AnalyticsResponse>(&json)
                .expect("Should get analytics response")
                .analytics;

            assert_eq!(
                vec![FetchedMetric::Count(1), FetchedMetric::Count(3)],
                fetched_analytics
                    .iter()
                    .map(|fetched| fetched.value)
                    .collect::<Vec<_>>(),
            );
        }
        // test for advertiser
        {
            let req = Request::builder()
                .extension(advertiser_auth.clone())
                .uri(format!(
                    "http://127.0.0.1/v5/analytics/for-advertiser?{}",
                    base_query
                ))
                .body(Body::empty())
                .expect("Should build Request");

            let analytics_response = analytics_router(req, &app)
                .await
                .expect("Should get analytics data");
            let json = hyper::body::to_bytes(analytics_response.into_body())
                .await
                .expect("Should get json");

            let fetched_analytics = serde_json::from_slice::<AnalyticsResponse>(&json)
                .expect("Should get analytics response")
                .analytics;

            assert_eq!(
                vec![FetchedMetric::Count(1), FetchedMetric::Count(5)],
                fetched_analytics
                    .iter()
                    .map(|fetched| fetched.value)
                    .collect::<Vec<_>>(),
            );
        }
        // test for admin
        {
            let req = Request::builder()
                .extension(admin_auth.clone())
                .uri(format!(
                    "http://127.0.0.1/v5/analytics/for-admin?{}",
                    base_query
                ))
                .body(Body::empty())
                .expect("Should build Request");

            let analytics_response = analytics_router(req, &app)
                .await
                .expect("Should get analytics data");
            let json = hyper::body::to_bytes(analytics_response.into_body())
                .await
                .expect("Should get json");

            let fetched_analytics = serde_json::from_slice::<AnalyticsResponse>(&json)
                .expect("Should get analytics response")
                .analytics;
            assert_eq!(
                vec![FetchedMetric::Count(1), FetchedMetric::Count(5)],
                fetched_analytics
                    .iter()
                    .map(|fetched| fetched.value)
                    .collect::<Vec<_>>(),
            );
        }

        // test for admin with all optional keys
        {
            let start_date = base_datehour - 72;
            let end_date = base_datehour - 1;
            let query = AnalyticsQuery {
                limit: 1000,
                event_type: CLICK,
                metric: Metric::Count,
                segment_by: Some(AllowedKey::Country),
                time: Time {
                    timeframe: Timeframe::Day,
                    start: start_date,
                    end: Some(end_date),
                },
                campaign_id: Some(DUMMY_CAMPAIGN.id),
                ad_unit: Some(DUMMY_IPFS[0]),
                ad_slot: Some(DUMMY_IPFS[1]),
                ad_slot_type: Some("TEST_TYPE".into()),
                advertiser: Some(*ADVERTISER),
                publisher: Some(*PUBLISHER),
                hostname: Some("localhost".into()),
                country: Some("Bulgaria".into()),
                os_name: Some(OperatingSystem::map_os("Windows")),
                chains: vec![GANACHE_1337.chain_id],
            };
            let query = serde_qs::to_string(&query).expect("should parse query");
            let req = Request::builder()
                .extension(admin_auth.clone())
                .uri(format!("http://127.0.0.1/v5/analytics/for-admin?{}", query))
                .body(Body::empty())
                .expect("Should build Request");

            let analytics_response = analytics_router(req, &app)
                .await
                .expect("Should get analytics data");
            let json = hyper::body::to_bytes(analytics_response.into_body())
                .await
                .expect("Should get json");

            let fetched_analytics = serde_json::from_slice::<AnalyticsResponse>(&json)
                .expect("Should get analytics response")
                .analytics;
            assert_eq!(1, fetched_analytics.len());
            assert_eq!(
                FetchedMetric::Count(1),
                fetched_analytics.get(0).unwrap().value,
            );
        }

        // test for admin with a different chain
        {
            let req = Request::builder()
                .extension(admin_auth_other_chain.clone())
                .uri(format!(
                    "http://127.0.0.1/v5/analytics/for-admin?{}",
                    base_query
                ))
                .body(Body::empty())
                .expect("Should build Request");

            let analytics_response = analytics_router(req, &app)
                .await
                .expect("Should get analytics data");
            let json = hyper::body::to_bytes(analytics_response.into_body())
                .await
                .expect("Should get json");

            let fetched_analytics = serde_json::from_slice::<AnalyticsResponse>(&json)
                .expect("Should get analytics response")
                .analytics;
            assert_eq!(
                vec![FetchedMetric::Count(69)],
                fetched_analytics
                    .iter()
                    .map(|fetched| fetched.value)
                    .collect::<Vec<_>>(),
            );
        }

        // TODO: Move test to a analytics_router test
        // test with no authUid
        // let req = Request::builder()
        //     .uri("http://127.0.0.1/v5/analytics?limit=100&eventType=CLICK&metric=count&timeframe=day")
        //     .body(Body::empty())
        //     .expect("Should build Request");

        // let analytics_response = analytics_router(req, &app, None, Some(AuthenticateAs::Publisher())).await;
        // let err_msg = "auth_as_key is provided but there is no Auth object".to_string();
        // assert!(matches!(
        //     analytics_response,
        //     Err(ResponseError::BadRequest(err_msg))
        // ));
    }
}
