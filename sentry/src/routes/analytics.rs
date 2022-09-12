//! `/v5/analytics` routes
//!

use std::{collections::HashSet, sync::Arc};

use axum::{Extension, Json};
use once_cell::sync::Lazy;

use adapter::client::Locked;
use primitives::{
    analytics::{
        query::{AllowedKey, ALLOWED_KEYS},
        AnalyticsQuery, AuthenticateAs,
    },
    sentry::AnalyticsResponse,
};

use crate::{
    application::Qs, db::analytics::fetch_analytics, response::ResponseError, Application, Auth,
};

/// The GET `/v5/analytics` allowed keys that are applied to the route.
pub static GET_ANALYTICS_ALLOWED_KEYS: Lazy<HashSet<AllowedKey>> = Lazy::new(|| {
    [AllowedKey::Country, AllowedKey::AdSlotType]
        .into_iter()
        .collect::<HashSet<_>>()
});

/// GET `/v5/analytics` routes
/// Request query parameters: [`AnalyticsQuery`].
///
/// Response: [`AnalyticsResponse`]
///
/// Analytics routes:
/// - GET `/v5/analytics`
/// - GET `/v5/analytics/for-publisher`
/// - GET `/v5/analytics/for-advertiser`
/// - GET `/v5/analytics/for-admin`
pub async fn get_analytics<C: Locked + 'static>(
    Extension(app): Extension<Arc<Application<C>>>,
    auth: Option<Extension<Auth>>,
    Extension(route_allowed_keys): Extension<HashSet<AllowedKey>>,
    authenticate_as: Option<Extension<AuthenticateAs>>,
    Qs(mut query): Qs<AnalyticsQuery>,
) -> Result<Json<AnalyticsResponse>, ResponseError> {
    // If we have a route that requires authentication the Chain will be extracted
    // from the sentry's authentication, which guarantees the value will exist
    // This will also override a query parameter for the chain if it is provided
    if let Some(Extension(auth)) = auth {
        query.chains = vec![auth.chain.chain_id]
    }

    let applied_limit = query.limit.min(app.config.analytics_find_limit);

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

    let analytics = match tokio::time::timeout(
        app.config.analytics_maxtime,
        fetch_analytics(
            &app.pool,
            query.clone(),
            route_allowed_keys,
            authenticate_as.map(|extension| extension.0),
            applied_limit,
        ),
    )
    .await
    {
        Ok(Ok(analytics)) => AnalyticsResponse { analytics },
        // Error getting the analytics
        Ok(Err(err)) => return Err(err.into()),
        // Timeout error
        Err(_elapsed) => {
            return Err(ResponseError::BadRequest(
                "Timeout when fetching analytics data".into(),
            ))
        }
    };

    Ok(Json(analytics))
}

#[cfg(test)]
mod test {
    use crate::{
        application::Qs,
        db::{analytics::update_analytics, DbPool},
        response::ResponseError,
        test_util::setup_dummy_app,
        Auth,
    };

    use super::*;
    use adapter::ethereum::test_util::{GANACHE_1, GANACHE_1337};
    use chrono::{Datelike, Utc};
    use primitives::{
        analytics::{
            query::{AllowedKey, Time},
            AnalyticsQuery, Metric, OperatingSystem, Timeframe,
        },
        sentry::{DateHour, FetchedMetric, UpdateAnalytics, CLICK, IMPRESSION},
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
        let app_guard = setup_dummy_app().await;
        let app = Extension(Arc::new(app_guard.app.clone()));

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

            let analytics_response = get_analytics(
                app.clone(),
                None,
                Extension(GET_ANALYTICS_ALLOWED_KEYS.clone()),
                None,
                Qs(AnalyticsQuery::default()),
            )
            .await
            .expect("Should get analytics data");

            let fetched_analytics = analytics_response.0.analytics;

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
                chains: vec![GANACHE_1337.chain_id],
                ..Default::default()
            };

            let analytics_response = get_analytics(
                app.clone(),
                None,
                Extension(GET_ANALYTICS_ALLOWED_KEYS.clone()),
                None,
                Qs(query),
            )
            .await
            .expect("Should get analytics data");

            let fetched_analytics = analytics_response.0.analytics;

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
                chains: vec![GANACHE_1337.chain_id],
                ..Default::default()
            };

            let analytics_response = get_analytics(
                app.clone(),
                None,
                Extension(GET_ANALYTICS_ALLOWED_KEYS.clone()),
                None,
                Qs(query),
            )
            .await
            .expect("Should get analytics data");

            let fetched_analytics = analytics_response.0.analytics;

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
                chains: vec![GANACHE_1337.chain_id],
                ..Default::default()
            };

            let analytics_response = get_analytics(
                app.clone(),
                None,
                Extension(GET_ANALYTICS_ALLOWED_KEYS.clone()),
                None,
                Qs(query),
            )
            .await
            .expect("Should get analytics data");

            let fetched_analytics = analytics_response.0.analytics;

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
                country: Some("Bulgaria".into()),
                chains: vec![GANACHE_1337.chain_id],
                ..Default::default()
            };

            let analytics_response = get_analytics(
                app.clone(),
                None,
                Extension(GET_ANALYTICS_ALLOWED_KEYS.clone()),
                None,
                Qs(query),
            )
            .await
            .expect("Should get analytics data");

            let fetched_analytics = analytics_response.0.analytics;

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
            let query = AnalyticsQuery {
                // This segment_by key is not allowed for the default, unauthenticated request!
                segment_by: Some(AllowedKey::CampaignId),
                campaign_id: Some(DUMMY_CAMPAIGN.id),
                ..Default::default()
            };

            let analytics_response = get_analytics(
                app.clone(),
                None,
                Extension(GET_ANALYTICS_ALLOWED_KEYS.clone()),
                None,
                Qs(query),
            )
            .await
            .expect_err("Should disallow the segmentBy");

            assert_eq!(
                ResponseError::Forbidden("Disallowed segmentBy `campaignId`".into()),
                analytics_response,
            );
        }

        // test with not allowed key
        // event type: CLICK
        {
            let query = AnalyticsQuery {
                event_type: CLICK,
                // This key not allowed for the default, unauthenticated request!
                campaign_id: Some(DUMMY_CAMPAIGN.id),
                ..Default::default()
            };

            let analytics_response = get_analytics(
                app.clone(),
                None,
                Extension(GET_ANALYTICS_ALLOWED_KEYS.clone()),
                None,
                Qs(query),
            )
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
            let query = AnalyticsQuery {
                metric: Metric::Paid,
                ..Default::default()
            };

            let analytics_response = get_analytics(
                app.clone(),
                None,
                Extension(GET_ANALYTICS_ALLOWED_KEYS.clone()),
                None,
                Qs(query),
            )
            .await
            .expect("Should get analytics data");

            let fetched_analytics = analytics_response.0.analytics;

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
            let query = AnalyticsQuery {
                time: Time {
                    timeframe: Timeframe::Week,
                    ..Default::default()
                },
                ..Default::default()
            };

            let analytics_response = get_analytics(
                app.clone(),
                None,
                Extension(GET_ANALYTICS_ALLOWED_KEYS.clone()),
                None,
                Qs(query),
            )
            .await
            .expect("Should get analytics data");

            let fetched_analytics = analytics_response.0.analytics;

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
            let query = AnalyticsQuery {
                limit: 2,
                ..Default::default()
            };

            let analytics_response = get_analytics(
                app.clone(),
                None,
                Extension(GET_ANALYTICS_ALLOWED_KEYS.clone()),
                None,
                Qs(query),
            )
            .await
            .expect("Should get analytics data");

            let fetched_analytics = analytics_response.0.analytics;

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
            let query = AnalyticsQuery {
                time: Time {
                    timeframe: Timeframe::Month,
                    ..Default::default()
                },
                ..Default::default()
            };

            let analytics_response = get_analytics(
                app.clone(),
                None,
                Extension(GET_ANALYTICS_ALLOWED_KEYS.clone()),
                None,
                Qs(query),
            )
            .await
            .expect("Should get analytics data");

            let fetched_analytics = analytics_response.0.analytics;

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
            let query = AnalyticsQuery {
                time: Time {
                    timeframe: Timeframe::Year,
                    ..Default::default()
                },
                ..Default::default()
            };

            let analytics_response = get_analytics(
                app.clone(),
                None,
                Extension(GET_ANALYTICS_ALLOWED_KEYS.clone()),
                None,
                Qs(query),
            )
            .await
            .expect("Should get analytics data");

            let fetched_analytics = analytics_response.0.analytics;

            // because data is grouped by month, when it's the 1st day of the month
            // and the CI runs, this causes a failing test.
            let expected = if Utc::today().day() == 1 {
                vec![FetchedMetric::Count(58), FetchedMetric::Count(42)]
            } else {
                vec![FetchedMetric::Count(100)]
            };

            assert_eq!(
                expected,
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
                event_type: CLICK,
                time: Time {
                    timeframe: Timeframe::Day,
                    start: start_date,
                    end: Some(end_date),
                },
                ..Default::default()
            };

            let analytics_response = get_analytics(
                app.clone(),
                None,
                Extension(GET_ANALYTICS_ALLOWED_KEYS.clone()),
                None,
                Qs(query),
            )
            .await
            .expect("Should get analytics data");

            let fetched_analytics = analytics_response.0.analytics;

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
                event_type: CLICK,
                time: Time {
                    timeframe: Timeframe::Day,
                    start: base_datehour - 1,
                    end: None,
                },
                chains: vec![GANACHE_1.chain_id],
                ..Default::default()
            };

            let analytics_response = get_analytics(
                app.clone(),
                None,
                Extension(GET_ANALYTICS_ALLOWED_KEYS.clone()),
                None,
                Qs(query),
            )
            .await
            .expect("Should get analytics data");

            let fetched_analytics = analytics_response.0.analytics;

            assert_eq!(
                vec![FetchedMetric::Count(69)],
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
        let app_guard = setup_dummy_app().await;
        let app = Extension(Arc::new(app_guard.app.clone()));

        // 27.12.2021 23:00:00
        let base_datehour = DateHour::from_ymdh(2021, 12, 27, 23);
        let base_query = AnalyticsQuery {
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

        insert_mock_analytics_for_auth_routes(&app.pool, base_datehour).await;

        let publisher_auth = Extension(Auth {
            era: 0,
            uid: IDS[&PUBLISHER],
            chain: GANACHE_1337.clone(),
        });
        let advertiser_auth = Extension(Auth {
            era: 0,
            uid: IDS[&ADVERTISER],
            chain: GANACHE_1337.clone(),
        });
        let admin_auth = Extension(Auth {
            era: 0,
            uid: IDS[&LEADER],
            chain: GANACHE_1337.clone(),
        });
        let admin_auth_other_chain = Extension(Auth {
            era: 0,
            uid: IDS[&LEADER],
            chain: GANACHE_1.clone(),
        });

        // test for publisher
        {
            let analytics_response = get_analytics(
                app.clone(),
                Some(publisher_auth.clone()),
                Extension(ALLOWED_KEYS.clone()),
                Some(Extension(AuthenticateAs::Publisher(publisher_auth.uid))),
                Qs(base_query.clone()),
            )
            .await
            .expect("Should get analytics data");

            let fetched_analytics = analytics_response.0.analytics;

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
            let analytics_response = get_analytics(
                app.clone(),
                Some(advertiser_auth.clone()),
                Extension(ALLOWED_KEYS.clone()),
                Some(Extension(AuthenticateAs::Advertiser(advertiser_auth.uid))),
                Qs(base_query.clone()),
            )
            .await
            .expect("Should get analytics data");

            let fetched_analytics = analytics_response.0.analytics;

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
            let analytics_response = get_analytics(
                app.clone(),
                Some(admin_auth.clone()),
                Extension(ALLOWED_KEYS.clone()),
                None,
                Qs(base_query.clone()),
            )
            .await
            .expect("Should get analytics data");

            let fetched_analytics = analytics_response.0.analytics;

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

            let analytics_response = get_analytics(
                app.clone(),
                Some(admin_auth.clone()),
                Extension(ALLOWED_KEYS.clone()),
                None,
                Qs(query.clone()),
            )
            .await
            .expect("Should get analytics data");

            let fetched_analytics = analytics_response.0.analytics;

            assert_eq!(1, fetched_analytics.len());
            assert_eq!(
                FetchedMetric::Count(1),
                fetched_analytics.get(0).unwrap().value,
            );
        }

        // test for admin with a different chain
        {
            let analytics_response = get_analytics(
                app.clone(),
                Some(admin_auth_other_chain.clone()),
                Extension(ALLOWED_KEYS.clone()),
                None,
                Qs(base_query.clone()),
            )
            .await
            .expect("Should get analytics data");

            let fetched_analytics = analytics_response.0.analytics;

            assert_eq!(
                vec![FetchedMetric::Count(69)],
                fetched_analytics
                    .iter()
                    .map(|fetched| fetched.value)
                    .collect::<Vec<_>>(),
            );
        }
    }

    #[tokio::test]
    async fn test_allowed_keys_for_guest() {
        let app_guard = setup_dummy_app().await;
        let app = Arc::new(app_guard.app);

        let allowed_keys = GET_ANALYTICS_ALLOWED_KEYS.clone();
        let base_datehour = DateHour::from_ymdh(2022, 1, 17, 14);

        // Test for each allowed key
        // Country
        {
            let query = AnalyticsQuery {
                time: Time {
                    timeframe: Timeframe::Day,
                    start: base_datehour - 1,
                    end: None,
                },
                country: Some("Bulgaria".to_string()),
                ..Default::default()
            };
            let res = get_analytics(
                Extension(app.clone()),
                None,
                Extension(allowed_keys.clone()),
                None,
                Qs(query),
            )
            .await;
            assert!(res.is_ok());
        }
        // Ad Slot Type
        {
            let query = AnalyticsQuery {
                time: Time {
                    timeframe: Timeframe::Day,
                    start: base_datehour - 1,
                    end: None,
                },
                ad_slot_type: Some("legacy_300x100".to_string()),
                ..Default::default()
            };
            let res = get_analytics(
                Extension(app.clone()),
                None,
                Extension(allowed_keys.clone()),
                None,
                Qs(query),
            )
            .await;
            assert!(res.is_ok());
        }
        // Test each not allowed key
        // CampaignId
        {
            let query = AnalyticsQuery {
                time: Time {
                    timeframe: Timeframe::Day,
                    start: base_datehour - 1,
                    end: None,
                },
                campaign_id: Some(DUMMY_CAMPAIGN.id),
                ..Default::default()
            };
            let res = get_analytics(
                Extension(app.clone()),
                None,
                Extension(allowed_keys.clone()),
                None,
                Qs(query),
            )
            .await
            .expect_err("should be an error");
            assert_eq!(
                ResponseError::Forbidden("Disallowed query key `campaignId`".into()),
                res,
            );
        }
        // AdUnit
        {
            let query = AnalyticsQuery {
                time: Time {
                    timeframe: Timeframe::Day,
                    start: base_datehour - 1,
                    end: None,
                },
                ad_unit: Some(DUMMY_IPFS[0]),
                ..Default::default()
            };
            let res = get_analytics(
                Extension(app.clone()),
                None,
                Extension(allowed_keys.clone()),
                None,
                Qs(query),
            )
            .await
            .expect_err("should be an error");
            assert_eq!(
                ResponseError::Forbidden("Disallowed query key `adUnit`".into()),
                res,
            );
        }
        // AdSlot
        {
            let query = AnalyticsQuery {
                time: Time {
                    timeframe: Timeframe::Day,
                    start: base_datehour - 1,
                    end: None,
                },
                ad_slot: Some(DUMMY_IPFS[1]),
                ..Default::default()
            };
            let res = get_analytics(
                Extension(app.clone()),
                None,
                Extension(allowed_keys.clone()),
                None,
                Qs(query),
            )
            .await
            .expect_err("should be an error");
            assert_eq!(
                ResponseError::Forbidden("Disallowed query key `adSlot`".into()),
                res,
            );
        }
        // Advertiser
        {
            let query = AnalyticsQuery {
                time: Time {
                    timeframe: Timeframe::Day,
                    start: base_datehour - 1,
                    end: None,
                },
                advertiser: Some(*ADVERTISER),
                ..Default::default()
            };
            let res = get_analytics(
                Extension(app.clone()),
                None,
                Extension(allowed_keys.clone()),
                None,
                Qs(query),
            )
            .await
            .expect_err("should throw an error");
            assert_eq!(
                ResponseError::Forbidden("Disallowed query key `advertiser`".into()),
                res,
            );
        }
        // Publisher
        {
            let query = AnalyticsQuery {
                time: Time {
                    timeframe: Timeframe::Day,
                    start: base_datehour - 1,
                    end: None,
                },
                publisher: Some(*PUBLISHER),
                ..Default::default()
            };
            let res = get_analytics(
                Extension(app.clone()),
                None,
                Extension(allowed_keys.clone()),
                None,
                Qs(query),
            )
            .await
            .expect_err("should throw an error");
            assert_eq!(
                ResponseError::Forbidden("Disallowed query key `publisher`".into()),
                res,
            );
        }
        // Hostname
        {
            let query = AnalyticsQuery {
                time: Time {
                    timeframe: Timeframe::Day,
                    start: base_datehour - 1,
                    end: None,
                },
                hostname: Some("localhost".to_string()),
                ..Default::default()
            };
            let res = get_analytics(
                Extension(app.clone()),
                None,
                Extension(allowed_keys.clone()),
                None,
                Qs(query),
            )
            .await
            .expect_err("should throw an error");
            assert_eq!(
                ResponseError::Forbidden("Disallowed query key `hostname`".into()),
                res,
            );
        }
        // OsName
        {
            let query = AnalyticsQuery {
                time: Time {
                    timeframe: Timeframe::Day,
                    start: base_datehour - 1,
                    end: None,
                },
                os_name: Some(OperatingSystem::map_os("Windows")),
                ..Default::default()
            };
            let res = get_analytics(
                Extension(app.clone()),
                None,
                Extension(allowed_keys.clone()),
                None,
                Qs(query),
            )
            .await
            .expect_err("should throw an error");
            assert_eq!(
                ResponseError::Forbidden("Disallowed query key `osName`".into()),
                res,
            );
        }
    }
}
