use crate::{db::analytics::get_analytics, success_response, Application, Auth, ResponseError};
use chrono::{Duration, Utc, Timelike};
use hyper::{Body, Request, Response};
use once_cell::sync::Lazy;
use primitives::{
    adapter::Adapter,
    analytics::{AnalyticsQuery, Metric, AuthenticateAs, ANALYTICS_QUERY_LIMIT},
    sentry::{DateHour, FetchedAnalytics},
    UnifiedNum,
};

pub static ALLOWED_KEYS: Lazy<[String; 9]> = Lazy::new(|| {
    [
        "campaignId".to_string(),
        "adUnit".to_string(),
        "adSlot".to_string(),
        "adSlotType".to_string(),
        "advertiser".to_string(),
        "publisher".to_string(),
        "hostname".to_string(),
        "country".to_string(),
        "osName".to_string(),
    ]
});

pub async fn analytics<A: Adapter>(
    req: Request<Body>,
    app: &Application<A>,
    allowed_keys: Option<Vec<String>>,
    auth_as_key: Option<String>,
) -> Result<Response<Body>, ResponseError> {
    let query = serde_urlencoded::from_str::<AnalyticsQuery>(req.uri().query().unwrap_or(""))?;
    let period_in_hours = query.timeframe.to_hours();
    let start_date = match query.start {
        Some(start_date) => DateHour::try_from(start_date)?,
        None => DateHour::try_from(Utc::today().and_hms(Utc::now().hour(), 0, 0) - Duration::hours(period_in_hours))?,
    };

    let end_date = match query.end {
        Some(end_date) => Some(DateHour::try_from(end_date)?),
        None => None,
    };

    let applied_limit = query.limit.min(ANALYTICS_QUERY_LIMIT);

    let allowed_keys = match allowed_keys {
        Some(keys) => keys,
        None => ALLOWED_KEYS.to_vec(),
    };

    if let Some(segment_by) = &query.segment_by {
        if !allowed_keys.contains(segment_by) {
            return Err(ResponseError::BadRequest(
                "Disallowed segmentBy".to_string(),
            ));
        }
    }

    for (key, _) in query.available_keys() {
        if !allowed_keys.contains(&key) {
            return Err(ResponseError::BadRequest(format!(
                "disallowed key in query: {}",
                key
            )));
        }
    }

    let auth = req
        .extensions()
        .get::<Auth>();

    let auth_as = match (auth_as_key, auth) {
        (Some(auth_as_key), Some(auth)) => AuthenticateAs::try_from(&auth_as_key, auth.uid),
        (Some(_), None) => return Err(ResponseError::BadRequest("auth_as_key is provided but there is no Auth object".to_string())),
        _ => None
    };

    let analytics = get_analytics(
        &app.pool,
        start_date,
        end_date,
        &query,
        auth_as,
        applied_limit,
    )
    .await?;

    let output = split_entries_by_timeframe(analytics, period_in_hours, &query.metric, &query.segment_by);

    Ok(success_response(serde_json::to_string(&output)?))
}

fn split_entries_by_timeframe(mut analytics: Vec<FetchedAnalytics>, period_in_hours: i64, metric: &Metric, segment: &Option<String>) -> Vec<FetchedAnalytics> {
    let mut res: Vec<FetchedAnalytics> = vec![];
    let period_in_hours = period_in_hours as usize;
    while analytics.len() > period_in_hours {
        let drain_index = analytics.len() - period_in_hours;
        let analytics_fraction: Vec<FetchedAnalytics> = analytics.drain(drain_index..).collect();
        let merged_analytics = merge_analytics(analytics_fraction, metric, segment);
        res.push(merged_analytics);
    }

    if analytics.len() > 0 {
        let merged_analytics = merge_analytics(analytics, metric, segment);
        res.push(merged_analytics);
    }

    res
}

fn merge_analytics(analytics: Vec<FetchedAnalytics>, metric: &Metric, segment: &Option<String>) -> FetchedAnalytics {
    let mut count = 0;
    let amount = UnifiedNum::from_u64(0);
    match metric {
        Metric::Count => {
            analytics.iter().for_each(|a| count += a.payout_count.unwrap());
            FetchedAnalytics {
                time: analytics.iter().nth(0).unwrap().time,
                payout_count: Some(count),
                payout_amount: None,
                segment: segment.clone(),
            }
        },
        Metric::Paid => {
            analytics.iter().for_each(|a| {
                amount.checked_add(&a.payout_amount.unwrap()).unwrap();
            });
            FetchedAnalytics {
                time: analytics.iter().nth(0).unwrap().time,
                payout_count: None,
                payout_amount: Some(amount),
                segment: segment.clone(),
            }
        }
    }
}

// async fn cache(
//     redis: &MultiplexedConnection,
//     key: String,
//     value: &str,
//     timeframe: i32,
//     logger: &Logger,
// ) {
//     if let Err(err) = redis::cmd("SETEX")
//         .arg(&key)
//         .arg(timeframe)
//         .arg(value)
//         .query_async::<_, ()>(&mut redis.clone())
//         .await
//     {
//         error!(&logger, "Server error: {}", err; "module" => "analytics-cache");
//     }
// }

#[cfg(test)]
mod test {
    use super::*;
    use primitives::{
        sentry::UpdateAnalytics,
        analytics::OperatingSystem,
        util::tests::prep_db::{
            DUMMY_CAMPAIGN, ADDRESSES
        },
    };
    use crate::
    {
        test_util::setup_dummy_app,
        routes::analytics::analytics,
        db::{
            analytics::update_analytics,
            tests_postgres::{setup_test_migrations, DATABASE_POOL},
            DbPool
        }
    };

    async fn insert_mock_analytics(pool: &DbPool) {
        // analytics for NOW
        let now_date = Utc::today().and_hms(1, 0, 0);
        let analytics_now = UpdateAnalytics {
            time: DateHour::try_from(now_date).expect("should parse"),
            campaign_id: DUMMY_CAMPAIGN.id,
            ad_unit: None,
            ad_slot: None,
            ad_slot_type: None,
            advertiser: ADDRESSES["creator"],
            publisher: ADDRESSES["publisher"],
            hostname: None,
            country: Some("Bulgaria".to_string()),
            os_name: OperatingSystem::map_os("Windows"),
            event_type: "CLICK".to_string(),
            amount_to_add: UnifiedNum::from_u64(1_000_000),
            count_to_add: 1,
        };
        update_analytics(pool, analytics_now).await.expect("Should update analytics");

        let analytics_now_different_country = UpdateAnalytics {
            time: DateHour::try_from(now_date).expect("should parse"),
            campaign_id: DUMMY_CAMPAIGN.id,
            ad_unit: None,
            ad_slot: None,
            ad_slot_type: None,
            advertiser: ADDRESSES["creator"],
            publisher: ADDRESSES["publisher"],
            hostname: None,
            country: Some("Japan".to_string()),
            os_name: OperatingSystem::map_os("Windows"),
            event_type: "CLICK".to_string(),
            amount_to_add: UnifiedNum::from_u64(1_000_000),
            count_to_add: 1,
        };
        update_analytics(pool, analytics_now_different_country).await.expect("Should update analytics");

        let analytics_two_hours_ago = UpdateAnalytics {
            time: DateHour::try_from(now_date - Duration::hours(2)).expect("should parse"),
            campaign_id: DUMMY_CAMPAIGN.id,
            ad_unit: None,
            ad_slot: None,
            ad_slot_type: None,
            advertiser: ADDRESSES["creator"],
            publisher: ADDRESSES["publisher"],
            hostname: None,
            country: Some("Bulgaria".to_string()),
            os_name: OperatingSystem::map_os("Windows"),
            event_type: "CLICK".to_string(),
            amount_to_add: UnifiedNum::from_u64(1_000_000),
            count_to_add: 1,
        };
        update_analytics(pool, analytics_two_hours_ago).await.expect("Should update analytics");

        let analytics_four_hours_ago = UpdateAnalytics {
            time: DateHour::try_from(now_date - Duration::hours(4)).expect("should parse"),
            campaign_id: DUMMY_CAMPAIGN.id,
            ad_unit: None,
            ad_slot: None,
            ad_slot_type: None,
            advertiser: ADDRESSES["creator"],
            publisher: ADDRESSES["publisher"],
            hostname: None,
            country: Some("Bulgaria".to_string()),
            os_name: OperatingSystem::map_os("Windows"),
            event_type: "CLICK".to_string(),
            amount_to_add: UnifiedNum::from_u64(1_000_000),
            count_to_add: 1,
        };
        update_analytics(pool, analytics_four_hours_ago).await.expect("Should update analytics");

        let analytics_three_days_ago = UpdateAnalytics {
            time: DateHour::try_from(now_date - Duration::days(3)).expect("should parse"),
            campaign_id: DUMMY_CAMPAIGN.id,
            ad_unit: None,
            ad_slot: None,
            ad_slot_type: None,
            advertiser: ADDRESSES["creator"],
            publisher: ADDRESSES["publisher"],
            hostname: None,
            country: Some("Bulgaria".to_string()),
            os_name: OperatingSystem::map_os("Windows"),
            event_type: "CLICK".to_string(),
            amount_to_add: UnifiedNum::from_u64(1_000_000),
            count_to_add: 1,
        };
        update_analytics(pool, analytics_three_days_ago).await.expect("Should update analytics");
        // analytics from 10 days ago
        let analytics_ten_days_ago = UpdateAnalytics {
            time: DateHour::try_from(now_date - Duration::days(10)).expect("should parse"),
            campaign_id: DUMMY_CAMPAIGN.id,
            ad_unit: None,
            ad_slot: None,
            ad_slot_type: None,
            advertiser: ADDRESSES["creator"],
            publisher: ADDRESSES["publisher"],
            hostname: None,
            country: Some("Bulgaria".to_string()),
            os_name: OperatingSystem::map_os("Windows"),
            event_type: "CLICK".to_string(),
            amount_to_add: UnifiedNum::from_u64(1_000_000),
            count_to_add: 1,
        };
        update_analytics(pool, analytics_ten_days_ago).await.expect("Should update analytics");

        let analytics_sixty_days_ago = UpdateAnalytics {
            time: DateHour::try_from(now_date - Duration::days(60)).expect("should parse"),
            campaign_id: DUMMY_CAMPAIGN.id,
            ad_unit: None,
            ad_slot: None,
            ad_slot_type: None,
            advertiser: ADDRESSES["creator"],
            publisher: ADDRESSES["publisher"],
            hostname: None,
            country: Some("Bulgaria".to_string()),
            os_name: OperatingSystem::map_os("Windows"),
            event_type: "CLICK".to_string(),
            amount_to_add: UnifiedNum::from_u64(1_000_000),
            count_to_add: 1,
        };
        update_analytics(pool, analytics_sixty_days_ago).await.expect("Should update analytics");

        let analytics_two_years_ago = UpdateAnalytics {
            time: DateHour::try_from(now_date - Duration::weeks(104)).expect("should parse"),
            campaign_id: DUMMY_CAMPAIGN.id,
            ad_unit: None,
            ad_slot: None,
            ad_slot_type: None,
            advertiser: ADDRESSES["creator"],
            publisher: ADDRESSES["publisher"],
            hostname: None,
            country: Some("Bulgaria".to_string()),
            os_name: OperatingSystem::map_os("Windows"),
            event_type: "CLICK".to_string(),
            amount_to_add: UnifiedNum::from_u64(1_000_000),
            count_to_add: 1,
        };
        update_analytics(pool, analytics_two_years_ago).await.expect("Should update analytics");
    }

    #[tokio::test]
    async fn test_analytics_route_for_guest() {
        let database = DATABASE_POOL.get().await.expect("Should get a DB pool");
        let app = setup_dummy_app().await;

        setup_test_migrations(database.pool.clone())
            .await
            .expect("Migrations should succeed");

        insert_mock_analytics(&database.pool).await;

        // Test with no optional values
        let req = Request::builder()
                .uri("http://127.0.0.1/analytics?limit=100&eventType=CLICK&metric=count&timeframe=day")
                .body(Body::empty())
                .expect("Should build Request");

        let analytics_response = analytics(req, &app, Some(vec!["country".into(), "ad_slot_type".into()]), None).await.expect("Should get analytics data");
        let json = hyper::body::to_bytes(analytics_response.into_body())
            .await
            .expect("Should get json");

        let fetched_analytics: FetchedAnalytics =
            serde_json::from_slice(&json).expect("Should get analytics response");
        assert!(fetched_analytics.payout_count.is_some());
        assert_eq!(fetched_analytics.payout_count.unwrap(), 4);
        // Test with start date

        let start_date = Utc::today().and_hms(Utc::now().hour(), 0, 0) - Duration::hours(1);
        let req = Request::builder()
            .uri(format!("http://127.0.0.1/analytics?limit=100&eventType=CLICK&metric=count&timeframe=day&start={}", start_date))
            .body(Body::empty())
            .expect("Should build Request");

        let analytics_response = analytics(req, &app, Some(vec!["country".into(), "ad_slot_type".into()]), None).await.expect("Should get analytics data");
        let json = hyper::body::to_bytes(analytics_response.into_body())
            .await
            .expect("Should get json");

        let fetched_analytics: FetchedAnalytics =
            serde_json::from_slice(&json).expect("Should get analytics response");
        assert_eq!(fetched_analytics.payout_count.unwrap(), 2);

        // Test with end date
        let end_date = Utc::today().and_hms(Utc::now().hour(), 0, 0) - Duration::hours(1);
        let req = Request::builder()
            .uri(format!("http://127.0.0.1/analytics?limit=100&eventType=CLICK&metric=count&timeframe=day&end={}", end_date))
            .body(Body::empty())
            .expect("Should build Request");

        let analytics_response = analytics(req, &app, Some(vec!["country".into(), "ad_slot_type".into()]), None).await.expect("Should get analytics data");
        let json = hyper::body::to_bytes(analytics_response.into_body())
            .await
            .expect("Should get json");

        let fetched_analytics: FetchedAnalytics =
            serde_json::from_slice(&json).expect("Should get analytics response");
        assert_eq!(fetched_analytics.payout_count.unwrap(), 3);

        // Test with start_date and end_date
        let start_date = Utc::today().and_hms(Utc::now().hour(), 0, 0) - Duration::hours(72);
        let end_date = Utc::today().and_hms(Utc::now().hour(), 0, 0) - Duration::hours(1);
        let req = Request::builder()
            .uri(format!("http://127.0.0.1/analytics?limit=100&eventType=CLICK&metric=count&timeframe=day&start={}&end={}", start_date, end_date))
            .body(Body::empty())
            .expect("Should build Request");

        let analytics_response = analytics(req, &app, Some(vec!["country".into(), "ad_slot_type".into()]), None).await.expect("Should get analytics data");
        let json = hyper::body::to_bytes(analytics_response.into_body())
            .await
            .expect("Should get json");

        let fetched_analytics: FetchedAnalytics =
            serde_json::from_slice(&json).expect("Should get analytics response");
        assert_eq!(fetched_analytics.payout_count.unwrap(), 2);
        // Test with segment_by
        // test with not allowed segment by
        // test with not allowed key
        // test with different metric
    }
}