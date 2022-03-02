//! `/v5/analytics` routes
//!

use std::collections::HashSet;

use crate::{db::analytics::get_analytics, success_response, Application, ResponseError};
use adapter::client::Locked;
use hyper::{Body, Request, Response};
use primitives::analytics::{
    query::{AllowedKey, ALLOWED_KEYS},
    AnalyticsQuery, AuthenticateAs,
};

/// `GET /v5/analytics` request
/// with query parameters: [`primitives::analytics::AnalyticsQuery`].
pub async fn analytics<C: Locked + 'static>(
    req: Request<Body>,
    app: &Application<C>,
    request_allowed: Option<HashSet<AllowedKey>>,
    authenticate_as: Option<AuthenticateAs>,
) -> Result<Response<Body>, ResponseError> {
    let query = serde_urlencoded::from_str::<AnalyticsQuery>(req.uri().query().unwrap_or(""))?;

    let applied_limit = query.limit.min(app.config.analytics_find_limit);

    let route_allowed_keys: HashSet<AllowedKey> =
        request_allowed.unwrap_or_else(|| ALLOWED_KEYS.clone());

    if let Some(segment_by) = query.segment_by {
        if !route_allowed_keys.contains(&segment_by) {
            return Err(ResponseError::Forbidden(format!(
                "Disallowed segmentBy `{}`",
                segment_by.to_camelCase()
            )));
        }
    }

    // for all `ALLOWED_KEYS` check if we have a value passed and if so,
    // cross check it with the allowed keys for the route
    let disallowed_key = ALLOWED_KEYS.iter().find(|allowed| {
        let in_query = query.get_key(**allowed).is_some();

        // is there a value for this key in the query
        // but not allowed for this route
        // return the key
        in_query && !route_allowed_keys.contains(*allowed)
    });

    // Return a error if value passed in the query is not allowed for this route
    if let Some(disallowed_key) = disallowed_key {
        return Err(ResponseError::Forbidden(format!(
            "Disallowed query key `{}`",
            disallowed_key.to_camelCase()
        )));
    }

    let analytics_maxtime = std::time::Duration::from_millis(app.config.analytics_maxtime.into());

    let analytics = match tokio::time::timeout(
        analytics_maxtime,
        get_analytics(
            &app.pool,
            query.clone(),
            route_allowed_keys,
            authenticate_as,
            applied_limit,
        ),
    )
    .await
    {
        Ok(Ok(analytics)) => analytics,
        // Error getting the analytics
        Ok(Err(err)) => return Err(err.into()),
        // Timeout error
        Err(_elapsed) => {
            return Err(ResponseError::BadRequest(
                "Timeout when fetching analytics data".into(),
            ))
        }
    };

    Ok(success_response(serde_json::to_string(&analytics)?))
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        db::{analytics::update_analytics, DbPool},
        routes::analytics::analytics,
        test_util::setup_dummy_app,
        Auth, ValidatorId,
    };
    use adapter::dummy::DUMMY_CHAIN;
    use chrono::{Timelike, Utc};
    use primitives::{
        analytics::{query::Time, Metric, OperatingSystem, Timeframe},
        sentry::{DateHour, FetchedAnalytics, FetchedMetric, UpdateAnalytics, CLICK, IMPRESSION},
        test_util::{ADVERTISER, CREATOR, PUBLISHER, PUBLISHER_2},
        test_util::{DUMMY_CAMPAIGN, DUMMY_IPFS},
        UnifiedNum,
    };

    async fn insert_mock_analytics(pool: &DbPool, base_datehour: DateHour<Utc>) {
        let analytics_now = UpdateAnalytics {
            time: base_datehour,
            campaign_id: DUMMY_CAMPAIGN.id,
            ad_unit: None,
            ad_slot: None,
            ad_slot_type: None,
            advertiser: *CREATOR,
            publisher: *PUBLISHER,
            hostname: None,
            country: Some("Bulgaria".to_string()),
            os_name: OperatingSystem::map_os("Windows"),
            event_type: CLICK,
            amount_to_add: UnifiedNum::from_u64(1_000_000),
            count_to_add: 1,
        };
        update_analytics(pool, analytics_now)
            .await
            .expect("Should update analytics");

        let analytics_now_different_country = UpdateAnalytics {
            time: base_datehour,
            campaign_id: DUMMY_CAMPAIGN.id,
            ad_unit: None,
            ad_slot: None,
            ad_slot_type: None,
            advertiser: *CREATOR,
            publisher: *PUBLISHER,
            hostname: None,
            country: Some("Japan".to_string()),
            os_name: OperatingSystem::map_os("Windows"),
            event_type: CLICK,
            amount_to_add: UnifiedNum::from_u64(1_000_000),
            count_to_add: 1,
        };
        update_analytics(pool, analytics_now_different_country)
            .await
            .expect("Should update analytics");

        let analytics_two_hours_ago = UpdateAnalytics {
            time: base_datehour - 2,
            campaign_id: DUMMY_CAMPAIGN.id,
            ad_unit: None,
            ad_slot: None,
            ad_slot_type: None,
            advertiser: *CREATOR,
            publisher: *PUBLISHER,
            hostname: None,
            country: Some("Bulgaria".to_string()),
            os_name: OperatingSystem::map_os("Windows"),
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
            ad_unit: None,
            ad_slot: None,
            ad_slot_type: None,
            advertiser: *CREATOR,
            publisher: *PUBLISHER,
            hostname: None,
            country: Some("Bulgaria".to_string()),
            os_name: OperatingSystem::map_os("Windows"),
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
            ad_unit: None,
            ad_slot: None,
            ad_slot_type: None,
            advertiser: *CREATOR,
            publisher: *PUBLISHER,
            hostname: None,
            country: Some("Bulgaria".to_string()),
            os_name: OperatingSystem::map_os("Windows"),
            event_type: CLICK,
            amount_to_add: UnifiedNum::from_u64(1_000_000),
            count_to_add: 1,
        };
        update_analytics(pool, analytics_three_days_ago)
            .await
            .expect("Should update analytics");
        // analytics from 10 days ago
        let analytics_ten_days_ago = UpdateAnalytics {
            time: base_datehour - (24 * 10),
            campaign_id: DUMMY_CAMPAIGN.id,
            ad_unit: None,
            ad_slot: None,
            ad_slot_type: None,
            advertiser: *CREATOR,
            publisher: *PUBLISHER,
            hostname: None,
            country: Some("Bulgaria".to_string()),
            os_name: OperatingSystem::map_os("Windows"),
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
            ad_unit: None,
            ad_slot: None,
            ad_slot_type: None,
            advertiser: *CREATOR,
            publisher: *PUBLISHER,
            hostname: None,
            country: Some("Bulgaria".to_string()),
            os_name: OperatingSystem::map_os("Windows"),
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
            ad_unit: None,
            ad_slot: None,
            ad_slot_type: None,
            advertiser: *CREATOR,
            publisher: *PUBLISHER,
            hostname: None,
            country: Some("Bulgaria".to_string()),
            os_name: OperatingSystem::map_os("Windows"),
            event_type: CLICK,
            amount_to_add: UnifiedNum::from_u64(1_000_000),
            count_to_add: 1,
        };
        update_analytics(pool, analytics_two_years_ago)
            .await
            .expect("Should update analytics");
    }

    #[tokio::test]
    async fn test_analytics_route_for_guest() {
        let app = setup_dummy_app().await;
        // analytics for NOW
        let now_datehour = DateHour::try_from(Utc::today().and_hms(Utc::now().hour(), 0, 0))
            .expect("should parse");
        insert_mock_analytics(&app.pool, now_datehour).await;

        // Test with no optional values
        {
            let req = Request::builder()
            .uri("http://127.0.0.1/v5/analytics?limit=100&eventType=CLICK&metric=count&timeframe=day")
            .body(Body::empty())
            .expect("Should build Request");

            let analytics_response = analytics(
                req,
                &app,
                Some(
                    vec![AllowedKey::Country, AllowedKey::AdSlotType]
                        .into_iter()
                        .collect(),
                ),
                None,
            )
            .await
            .expect("Should get analytics data");
            let json = hyper::body::to_bytes(analytics_response.into_body())
                .await
                .expect("Should get json");

            let fetched_analytics: Vec<FetchedAnalytics> =
                serde_json::from_slice(&json).expect("Should get analytics response");

            assert_eq!(
                vec![
                    FetchedMetric::Count(1),
                    FetchedMetric::Count(1),
                    FetchedMetric::Count(2)
                ],
                fetched_analytics
                    .iter()
                    .map(|fetched| fetched.value)
                    .collect::<Vec<_>>(),
                "Total of 4 count events for the 3 hours are expected"
            );
        }

        // Test with start date 1 hour ago
        {
            let query = AnalyticsQuery {
                limit: 1000,
                event_type: CLICK,
                metric: Metric::Count,
                segment_by: None,
                time: Time {
                    timeframe: Timeframe::Day,
                    start: now_datehour - 1,
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
            };
            let query = serde_urlencoded::to_string(query).expect("should parse query");
            let req = Request::builder()
                .uri(format!("http://127.0.0.1/v5/analytics?{}", query))
                .body(Body::empty())
                .expect("Should build Request");

            let analytics_response = analytics(
                req,
                &app,
                Some(
                    vec![AllowedKey::Country, AllowedKey::AdSlotType]
                        .into_iter()
                        .collect(),
                ),
                None,
            )
            .await
            .expect("Should get analytics data");
            let json = hyper::body::to_bytes(analytics_response.into_body())
                .await
                .expect("Should get json");

            let fetched_analytics: Vec<FetchedAnalytics> =
                serde_json::from_slice(&json).expect("Should get analytics response");
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
                    start: DateHour::now() - &Timeframe::Day,
                    end: Some(now_datehour - 1),
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
            };
            let query = serde_urlencoded::to_string(query).expect("should parse query");
            let req = Request::builder()
                .uri(format!("http://127.0.0.1/v5/analytics?{}", query))
                .body(Body::empty())
                .expect("Should build Request");

            let analytics_response = analytics(
                req,
                &app,
                Some(
                    vec![AllowedKey::Country, AllowedKey::AdSlotType]
                        .into_iter()
                        .collect(),
                ),
                None,
            )
            .await
            .expect("Should get analytics data");
            let json = hyper::body::to_bytes(analytics_response.into_body())
                .await
                .expect("Should get json");

            let fetched_analytics: Vec<FetchedAnalytics> =
                serde_json::from_slice(&json).expect("Should get analytics response");

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
                    start: now_datehour - 72,
                    // subtract 1 hour
                    end: Some(now_datehour - 1),
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
            };
            let query = serde_urlencoded::to_string(query).expect("should serialize query");
            let req = Request::builder()
                .uri(format!("http://127.0.0.1/v5/analytics?{}", query))
                .body(Body::empty())
                .expect("Should build Request");
            let analytics_response = analytics(
                req,
                &app,
                Some(
                    vec![AllowedKey::Country, AllowedKey::AdSlotType]
                        .into_iter()
                        .collect(),
                ),
                None,
            )
            .await
            .expect("Should get analytics data");
            let json = hyper::body::to_bytes(analytics_response.into_body())
                .await
                .expect("Should get json");

            let fetched_analytics: Vec<FetchedAnalytics> =
                serde_json::from_slice(&json).expect("Should get analytics response");

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
                    start: DateHour::now() - &Timeframe::Day,
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
            };
            let query = serde_urlencoded::to_string(query).expect("should parse query");
            let req = Request::builder()
                .uri(format!("http://127.0.0.1/v5/analytics?{}", query))
                .body(Body::empty())
                .expect("Should build Request");

            let analytics_response = analytics(
                req,
                &app,
                Some(
                    vec![AllowedKey::Country, AllowedKey::AdSlotType]
                        .into_iter()
                        .collect(),
                ),
                None,
            )
            .await
            .expect("Should get analytics data");
            let json = hyper::body::to_bytes(analytics_response.into_body())
                .await
                .expect("Should get json");

            let fetched_analytics: Vec<FetchedAnalytics> =
                serde_json::from_slice(&json).expect("Should get analytics response");

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
        {
            let req = Request::builder()
            .uri("http://127.0.0.1/v5/analytics?limit=100&eventType=CLICK&metric=count&timeframe=day&segmentBy=campaignId&campaignId=0x936da01f9abd4d9d80c702af85c822a8")
            .body(Body::empty())
            .expect("Should build Request");

            let analytics_response = analytics(
                req,
                &app,
                Some(
                    vec![AllowedKey::Country, AllowedKey::AdSlotType]
                        .into_iter()
                        .collect(),
                ),
                None,
            )
            .await
            .expect_err("Should result in segmentBy error");

            assert_eq!(
                ResponseError::Forbidden("Disallowed segmentBy `campaignId`".into()),
                analytics_response,
            );
        }

        // test with not allowed key
        {
            let req = Request::builder()
            .uri("http://127.0.0.1/v5/analytics?limit=100&eventType=CLICK&metric=count&timeframe=day&campaignId=0x936da01f9abd4d9d80c702af85c822a8")
            .body(Body::empty())
            .expect("Should build Request");

            let analytics_response = analytics(
                req,
                &app,
                Some(
                    vec![AllowedKey::Country, AllowedKey::AdSlotType]
                        .into_iter()
                        .collect(),
                ),
                None,
            )
            .await
            .expect_err("Should get error for disallowed key");

            assert_eq!(
                ResponseError::Forbidden("Disallowed query key `campaignId`".into()),
                analytics_response,
            );
        }

        // test with different metric
        {
            let req = Request::builder()
            .uri("http://127.0.0.1/v5/analytics?limit=100&eventType=CLICK&metric=paid&timeframe=day")
            .body(Body::empty())
            .expect("Should build Request");

            let analytics_response = analytics(
                req,
                &app,
                Some(
                    vec![AllowedKey::Country, AllowedKey::AdSlotType]
                        .into_iter()
                        .collect(),
                ),
                None,
            )
            .await
            .expect("Should get analytics data");
            let json = hyper::body::to_bytes(analytics_response.into_body())
                .await
                .expect("Should get json");

            let fetched_analytics: Vec<FetchedAnalytics> =
                serde_json::from_slice(&json).expect("Should get analytics response");

            assert_eq!(
                vec![
                    FetchedMetric::Paid(1_000_000.into()),
                    FetchedMetric::Paid(1_000_000.into()),
                    FetchedMetric::Paid(2_000_000.into())
                ],
                fetched_analytics
                    .iter()
                    .map(|fetched| fetched.value)
                    .collect::<Vec<_>>(),
            );
        }

        // Test with different timeframe
        {
            let req = Request::builder()
            .uri("http://127.0.0.1/v5/analytics?limit=100&eventType=CLICK&metric=count&timeframe=week")
            .body(Body::empty())
            .expect("Should build Request");

            let analytics_response = analytics(
                req,
                &app,
                Some(
                    vec![AllowedKey::Country, AllowedKey::AdSlotType]
                        .into_iter()
                        .collect(),
                ),
                None,
            )
            .await
            .expect("Should get analytics data");
            let json = hyper::body::to_bytes(analytics_response.into_body())
                .await
                .expect("Should get json");

            let fetched_analytics: Vec<FetchedAnalytics> =
                serde_json::from_slice(&json).expect("Should get analytics response");

            assert_eq!(
                vec![
                    FetchedMetric::Count(1),
                    FetchedMetric::Count(1),
                    FetchedMetric::Count(1),
                    FetchedMetric::Count(2)
                ],
                fetched_analytics
                    .iter()
                    .map(|fetched| fetched.value)
                    .collect::<Vec<_>>(),
            );
        }

        // Test with a limit
        {
            let req = Request::builder()
                .uri(
                    "http://127.0.0.1/v5/analytics?limit=2&eventType=CLICK&metric=count&timeframe=day",
                )
                .body(Body::empty())
                .expect("Should build Request");

            let analytics_response = analytics(
                req,
                &app,
                Some(
                    vec![AllowedKey::Country, AllowedKey::AdSlotType]
                        .into_iter()
                        .collect(),
                ),
                None,
            )
            .await
            .expect("Should get analytics data");
            let json = hyper::body::to_bytes(analytics_response.into_body())
                .await
                .expect("Should get json");

            let fetched_analytics: Vec<FetchedAnalytics> =
                serde_json::from_slice(&json).expect("Should get analytics response");

            assert_eq!(
                vec![FetchedMetric::Count(1), FetchedMetric::Count(1),],
                fetched_analytics
                    .iter()
                    .map(|fetched| fetched.value)
                    .collect::<Vec<_>>(),
            );
        }

        // Test with a month timeframe
        {
            let req = Request::builder()
            .uri(
                "http://127.0.0.1/v5/analytics?limit=100&eventType=CLICK&metric=count&timeframe=month",
            )
            .body(Body::empty())
            .expect("Should build Request");

            let analytics_response = analytics(
                req,
                &app,
                Some(
                    vec![AllowedKey::Country, AllowedKey::AdSlotType]
                        .into_iter()
                        .collect(),
                ),
                None,
            )
            .await
            .expect("Should get analytics data");
            let json = hyper::body::to_bytes(analytics_response.into_body())
                .await
                .expect("Should get json");

            let fetched_analytics: Vec<FetchedAnalytics> =
                serde_json::from_slice(&json).expect("Should get analytics response");

            assert_eq!(
                vec![
                    FetchedMetric::Count(1),
                    FetchedMetric::Count(1),
                    FetchedMetric::Count(4)
                ],
                fetched_analytics
                    .iter()
                    .map(|fetched| fetched.value)
                    .collect::<Vec<_>>(),
            );
        }

        // Test with a year timeframe
        {
            let req = Request::builder()
            .uri("http://127.0.0.1/v5/analytics?limit=100&eventType=CLICK&metric=count&timeframe=year")
            .body(Body::empty())
            .expect("Should build Request");

            let analytics_response = analytics(
                req,
                &app,
                Some(
                    vec![AllowedKey::Country, AllowedKey::AdSlotType]
                        .into_iter()
                        .collect(),
                ),
                None,
            )
            .await
            .expect("Should get analytics data");
            let json = hyper::body::to_bytes(analytics_response.into_body())
                .await
                .expect("Should get json");

            let fetched_analytics: Vec<FetchedAnalytics> =
                serde_json::from_slice(&json).expect("Should get analytics response");

            assert_eq!(
                vec![FetchedMetric::Count(1), FetchedMetric::Count(6),],
                fetched_analytics
                    .iter()
                    .map(|fetched| fetched.value)
                    .collect::<Vec<_>>(),
            );
        }

        // Test with start and end as timestamps
        {
            let start_date = DateHour::<Utc>::now() - 72;
            // subtract 1 hour
            let end_date = DateHour::<Utc>::now() - 1;
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
            };
            let query = serde_urlencoded::to_string(query).expect("should parse query");
            let req = Request::builder()
                .uri(format!("http://127.0.0.1/v5/analytics?{}", query))
                .body(Body::empty())
                .expect("Should build Request");
            let analytics_response = analytics(
                req,
                &app,
                Some(
                    vec![AllowedKey::Country, AllowedKey::AdSlotType]
                        .into_iter()
                        .collect(),
                ),
                None,
            )
            .await
            .expect("Should get analytics data");
            let json = hyper::body::to_bytes(analytics_response.into_body())
                .await
                .expect("Should get json");

            let fetched_analytics: Vec<FetchedAnalytics> =
                serde_json::from_slice(&json).expect("Should get analytics response");

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
        // Test with timeframe=day and start_date= 2 or more days ago to check if the results vec is split properly
    }

    async fn insert_mock_analytics_for_auth_routes(pool: &DbPool) {
        // Analytics with publisher and advertiser
        let now_date = DateHour::try_from(Utc::today().and_hms(Utc::now().hour(), 0, 0))
            .expect("should parse");
        let analytics = UpdateAnalytics {
            time: now_date,
            campaign_id: DUMMY_CAMPAIGN.id,
            ad_unit: Some(DUMMY_IPFS[0]),
            ad_slot: Some(DUMMY_IPFS[1]),
            ad_slot_type: None,
            advertiser: *CREATOR,
            publisher: *PUBLISHER,
            hostname: None,
            country: Some("Bulgaria".to_string()),
            os_name: OperatingSystem::map_os("Windows"),
            event_type: CLICK,
            amount_to_add: UnifiedNum::from_u64(1_000_000),
            count_to_add: 1,
        };
        update_analytics(pool, analytics)
            .await
            .expect("Should update analytics");
        // Analytics with a different unit/slot
        let analytics_different_slot_unit = UpdateAnalytics {
            time: now_date,
            campaign_id: DUMMY_CAMPAIGN.id,
            ad_unit: Some(DUMMY_IPFS[2]),
            ad_slot: Some(DUMMY_IPFS[3]),
            ad_slot_type: None,
            advertiser: *CREATOR,
            publisher: *PUBLISHER,
            hostname: None,
            country: Some("Bulgaria".to_string()),
            os_name: OperatingSystem::map_os("Windows"),
            event_type: CLICK,
            amount_to_add: UnifiedNum::from_u64(1_000_000),
            count_to_add: 1,
        };
        update_analytics(pool, analytics_different_slot_unit)
            .await
            .expect("Should update analytics");
        // Analytics with a different event type
        let analytics_different_event = UpdateAnalytics {
            time: now_date,
            campaign_id: DUMMY_CAMPAIGN.id,
            ad_unit: Some(DUMMY_IPFS[0]),
            ad_slot: Some(DUMMY_IPFS[1]),
            ad_slot_type: None,
            advertiser: *CREATOR,
            publisher: *PUBLISHER,
            hostname: None,
            country: Some("Bulgaria".to_string()),
            os_name: OperatingSystem::map_os("Windows"),
            event_type: IMPRESSION,
            amount_to_add: UnifiedNum::from_u64(1_000_000),
            count_to_add: 1,
        };
        update_analytics(pool, analytics_different_event)
            .await
            .expect("Should update analytics");
        // Analytics with no None fields
        let analytics_all_optional_fields = UpdateAnalytics {
            time: now_date - 2,
            campaign_id: DUMMY_CAMPAIGN.id,
            ad_unit: Some(DUMMY_IPFS[0]),
            ad_slot: Some(DUMMY_IPFS[1]),
            ad_slot_type: Some("TEST_TYPE".to_string()),
            advertiser: *CREATOR,
            publisher: *PUBLISHER,
            hostname: Some("localhost".to_string()),
            country: Some("Bulgaria".to_string()),
            os_name: OperatingSystem::map_os("Windows"),
            event_type: CLICK,
            amount_to_add: UnifiedNum::from_u64(1_000_000),
            count_to_add: 1,
        };
        update_analytics(pool, analytics_all_optional_fields)
            .await
            .expect("Should update analytics");
        // Analytics with different publisher
        let analytics_different_publisher = UpdateAnalytics {
            time: now_date,
            campaign_id: DUMMY_CAMPAIGN.id,
            ad_unit: Some(DUMMY_IPFS[0]),
            ad_slot: Some(DUMMY_IPFS[1]),
            ad_slot_type: None,
            advertiser: *CREATOR,
            publisher: *PUBLISHER_2,
            hostname: None,
            country: Some("Bulgaria".to_string()),
            os_name: OperatingSystem::map_os("Windows"),
            event_type: CLICK,
            amount_to_add: UnifiedNum::from_u64(1_000_000),
            count_to_add: 1,
        };
        update_analytics(pool, analytics_different_publisher)
            .await
            .expect("Should update analytics");
        // Analytics with different advertiser
        let analytics_different_advertiser = UpdateAnalytics {
            time: now_date,
            campaign_id: DUMMY_CAMPAIGN.id,
            ad_unit: Some(DUMMY_IPFS[0]),
            ad_slot: Some(DUMMY_IPFS[1]),
            ad_slot_type: None,
            advertiser: *ADVERTISER,
            publisher: *PUBLISHER,
            hostname: None,
            country: Some("Bulgaria".to_string()),
            os_name: OperatingSystem::map_os("Windows"),
            event_type: CLICK,
            amount_to_add: UnifiedNum::from_u64(1_000_000),
            count_to_add: 1,
        };
        update_analytics(pool, analytics_different_advertiser)
            .await
            .expect("Should update analytics");
        // Analytics with both a different publisher and advertiser
        let analytics_different_publisher_advertiser = UpdateAnalytics {
            time: now_date,
            campaign_id: DUMMY_CAMPAIGN.id,
            ad_unit: Some(DUMMY_IPFS[0]),
            ad_slot: Some(DUMMY_IPFS[1]),
            ad_slot_type: None,
            advertiser: *ADVERTISER,
            publisher: *PUBLISHER_2,
            hostname: None,
            country: Some("Bulgaria".to_string()),
            os_name: OperatingSystem::map_os("Windows"),
            event_type: CLICK,
            amount_to_add: UnifiedNum::from_u64(1_000_000),
            count_to_add: 1,
        };
        update_analytics(pool, analytics_different_publisher_advertiser)
            .await
            .expect("Should update analytics");
    }
    #[tokio::test]
    async fn test_analytics_route_with_auth() {
        let app = setup_dummy_app().await;
        insert_mock_analytics_for_auth_routes(&app.pool).await;

        let publisher_auth = Auth {
            era: 0,
            uid: ValidatorId::from(*PUBLISHER),
            chain: DUMMY_CHAIN.clone(),
        };
        let advertiser_auth = Auth {
            era: 0,
            uid: ValidatorId::from(*CREATOR),
            chain: DUMMY_CHAIN.clone(),
        };
        let admin_auth = Auth {
            era: 0,
            uid: ValidatorId::try_from("0xce07CbB7e054514D590a0262C93070D838bFBA2e")
                .expect("should create"),
            chain: DUMMY_CHAIN.clone(),
        };
        // test for publisher
        {
            let req = Request::builder()
            .extension(publisher_auth.clone())
            .uri("http://127.0.0.1/v5/analytics?limit=100&eventType=CLICK&metric=count&timeframe=day")
            .body(Body::empty())
            .expect("Should build Request");

            let analytics_response = analytics(
                req,
                &app,
                None,
                Some(AuthenticateAs::Publisher(publisher_auth.uid)),
            )
            .await
            .expect("Should get analytics data");
            let json = hyper::body::to_bytes(analytics_response.into_body())
                .await
                .expect("Should get json");

            let fetched_analytics: Vec<FetchedAnalytics> =
                serde_json::from_slice(&json).expect("Should get analytics response");
            assert_eq!(2, fetched_analytics.len());
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
            .uri("http://127.0.0.1/v5/analytics?limit=100&eventType=CLICK&metric=count&timeframe=day")
            .body(Body::empty())
            .expect("Should build Request");

            let analytics_response = analytics(
                req,
                &app,
                None,
                Some(AuthenticateAs::Advertiser(advertiser_auth.uid)),
            )
            .await
            .expect("Should get analytics data");
            let json = hyper::body::to_bytes(analytics_response.into_body())
                .await
                .expect("Should get json");

            let fetched_analytics: Vec<FetchedAnalytics> =
                serde_json::from_slice(&json).expect("Should get analytics response");
            assert_eq!(2, fetched_analytics.len());
            assert_eq!(
                vec![FetchedMetric::Count(1), FetchedMetric::Count(3)],
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
            .uri("http://127.0.0.1/v5/analytics?limit=100&eventType=CLICK&metric=count&timeframe=day")
            .body(Body::empty())
            .expect("Should build Request");

            let analytics_response = analytics(req, &app, None, None)
                .await
                .expect("Should get analytics data");
            let json = hyper::body::to_bytes(analytics_response.into_body())
                .await
                .expect("Should get json");

            let fetched_analytics: Vec<FetchedAnalytics> =
                serde_json::from_slice(&json).expect("Should get analytics response");
            assert_eq!(2, fetched_analytics.len());
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
            let start_date = DateHour::<Utc>::now() - 72;
            let end_date = DateHour::<Utc>::now() - 1;
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
                advertiser: Some(*CREATOR),
                publisher: Some(*PUBLISHER),
                hostname: Some("localhost".into()),
                country: Some("Bulgaria".into()),
                os_name: Some(OperatingSystem::map_os("Windows")),
            };
            let query = serde_urlencoded::to_string(query).expect("should parse query");
            let req = Request::builder()
                .uri(format!("http://127.0.0.1/v5/analytics?{}", query))
                .body(Body::empty())
                .expect("Should build Request");

            let analytics_response = analytics(req, &app, None, None)
                .await
                .expect("Should get analytics data");
            let json = hyper::body::to_bytes(analytics_response.into_body())
                .await
                .expect("Should get json");

            let fetched_analytics: Vec<FetchedAnalytics> =
                serde_json::from_slice(&json).expect("Should get analytics response");
            assert_eq!(1, fetched_analytics.len());
            assert_eq!(
                FetchedMetric::Count(1),
                fetched_analytics.get(0).unwrap().value,
            );
        }

        // TODO: Move test to a analytics_router test
        // test with no authUid
        // let req = Request::builder()
        //     .uri("http://127.0.0.1/v5/analytics?limit=100&eventType=CLICK&metric=count&timeframe=day")
        //     .body(Body::empty())
        //     .expect("Should build Request");

        // let analytics_response = analytics(req, &app, None, Some(AuthenticateAs::Publisher())).await;
        // let err_msg = "auth_as_key is provided but there is no Auth object".to_string();
        // assert!(matches!(
        //     analytics_response,
        //     Err(ResponseError::BadRequest(err_msg))
        // ));
    }
}
