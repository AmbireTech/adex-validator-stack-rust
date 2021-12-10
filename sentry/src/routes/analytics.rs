use crate::{db::analytics::get_analytics, success_response, Application, Auth, ResponseError};
use hyper::{Body, Request, Response};
use once_cell::sync::Lazy;
use primitives::{
    adapter::Adapter,
    analytics::{
        AnalyticsQuery, AnalyticsQueryTime, AuthenticateAs, Metric, ANALYTICS_QUERY_LIMIT,
    },
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
        Some(ref start_date) => start_date.to_owned(),
        None => AnalyticsQueryTime::Date(DateHour::now() - period_in_hours),
    };

    let applied_limit = query.limit.min(ANALYTICS_QUERY_LIMIT);

    let not_allowed_keys = match &allowed_keys {
        Some(keys) => ALLOWED_KEYS.iter().filter(|k| !keys.contains(k)).collect(),
        None => vec![],
    };

    if let Some(segment_by) = &query.segment_by {
        if not_allowed_keys.contains(&segment_by) {
            return Err(ResponseError::BadRequest(format!(
                "Disallowed segmentBy: {}",
                segment_by
            )));
        }
        if query.get_key(segment_by).is_none() {
            return Err(ResponseError::BadRequest(
                "SegmentBy is provided but a key is not passed".to_string(),
            ));
        }
    }

    for key in not_allowed_keys {
        if query.get_key(key).is_some() {
            return Err(ResponseError::BadRequest(format!(
                "disallowed key in query: {}",
                key
            )));
        }
    }

    let auth = req.extensions().get::<Auth>();

    let auth_as = match (auth_as_key, auth) {
        (Some(auth_as_key), Some(auth)) => AuthenticateAs::try_from(&auth_as_key, auth.uid),
        (Some(_), None) => {
            return Err(ResponseError::BadRequest(
                "auth_as_key is provided but there is no Auth object".to_string(),
            ))
        }
        _ => None,
    };

    let allowed_keys = allowed_keys.unwrap_or_else(|| ALLOWED_KEYS.to_vec());
    let analytics = get_analytics(
        &app.pool,
        &start_date,
        &query,
        allowed_keys,
        auth_as,
        applied_limit,
    )
    .await?;

    let output =
        split_entries_by_timeframe(analytics, period_in_hours, &query.metric, &query.segment_by);

    Ok(success_response(serde_json::to_string(&output)?))
}

fn split_entries_by_timeframe(
    mut analytics: Vec<FetchedAnalytics>,
    period_in_hours: i64,
    metric: &Metric,
    segment: &Option<String>,
) -> Vec<FetchedAnalytics> {
    let mut res: Vec<FetchedAnalytics> = vec![];
    let period_in_hours = period_in_hours as usize;
    // TODO: If there is an hour with no events this logic will fail
    // FIX BEFORE MERGE!
    while analytics.len() > period_in_hours {
        let drain_index = analytics.len() - period_in_hours;
        let analytics_fraction: Vec<FetchedAnalytics> = analytics.drain(drain_index..).collect();
        let merged_analytics = merge_analytics(analytics_fraction, metric, segment);
        res.push(merged_analytics);
    }

    if !analytics.is_empty() {
        let merged_analytics = merge_analytics(analytics, metric, segment);
        res.push(merged_analytics);
    }
    res
}

fn merge_analytics(
    analytics: Vec<FetchedAnalytics>,
    metric: &Metric,
    segment: &Option<String>,
) -> FetchedAnalytics {
    let mut count = 0;
    let amount = UnifiedNum::from_u64(0);
    match metric {
        Metric::Count => {
            analytics
                .iter()
                .for_each(|a| count += a.payout_count.unwrap());
            FetchedAnalytics {
                time: analytics.get(0).unwrap().time,
                payout_count: Some(count),
                payout_amount: None,
                segment: segment.clone(),
            }
        }
        Metric::Paid => {
            analytics.iter().for_each(|a| {
                amount.checked_add(&a.payout_amount.unwrap()).unwrap();
            });
            FetchedAnalytics {
                time: analytics.get(0).unwrap().time,
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
    use crate::{
        db::{analytics::update_analytics, DbPool},
        routes::analytics::analytics,
        test_util::setup_dummy_app,
        ValidatorId,
    };
    use chrono::Utc;
    use primitives::{
        analytics::{OperatingSystem, Timeframe},
        sentry::UpdateAnalytics,
        util::tests::prep_db::{ADDRESSES, DUMMY_CAMPAIGN, DUMMY_IPFS},
    };

    async fn insert_mock_analytics(pool: &DbPool) {
        // analytics for NOW
        let now_date = DateHour::try_from(Utc::today().and_hms(1, 0, 0)).expect("should parse");
        let analytics_now = UpdateAnalytics {
            time: now_date,
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
        update_analytics(pool, analytics_now)
            .await
            .expect("Should update analytics");

        let analytics_now_different_country = UpdateAnalytics {
            time: now_date,
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
        update_analytics(pool, analytics_now_different_country)
            .await
            .expect("Should update analytics");

        let analytics_two_hours_ago = UpdateAnalytics {
            time: now_date - 2,
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
        update_analytics(pool, analytics_two_hours_ago)
            .await
            .expect("Should update analytics");

        let analytics_four_hours_ago = UpdateAnalytics {
            time: now_date - 4,
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
        update_analytics(pool, analytics_four_hours_ago)
            .await
            .expect("Should update analytics");

        let analytics_three_days_ago = UpdateAnalytics {
            time: now_date - (24 * 3),
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
        update_analytics(pool, analytics_three_days_ago)
            .await
            .expect("Should update analytics");
        // analytics from 10 days ago
        let analytics_ten_days_ago = UpdateAnalytics {
            time: now_date - (24 * 10),
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
        update_analytics(pool, analytics_ten_days_ago)
            .await
            .expect("Should update analytics");

        let analytics_sixty_days_ago = UpdateAnalytics {
            time: now_date - (24 * 60),
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
        update_analytics(pool, analytics_sixty_days_ago)
            .await
            .expect("Should update analytics");

        let analytics_two_years_ago = UpdateAnalytics {
            time: now_date - (24 * 7 * 104),
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
        update_analytics(pool, analytics_two_years_ago)
            .await
            .expect("Should update analytics");
    }

    #[tokio::test]
    async fn test_analytics_route_for_guest() {
        let app = setup_dummy_app().await;

        insert_mock_analytics(&app.pool).await;

        // Test with no optional values
        let req = Request::builder()
            .uri("http://127.0.0.1/analytics?limit=100&eventType=CLICK&metric=count&timeframe=day")
            .body(Body::empty())
            .expect("Should build Request");

        let analytics_response = analytics(
            req,
            &app,
            Some(vec!["country".into(), "ad_slot_type".into()]),
            None,
        )
        .await
        .expect("Should get analytics data");
        let json = hyper::body::to_bytes(analytics_response.into_body())
            .await
            .expect("Should get json");

        let fetched_analytics: Vec<FetchedAnalytics> =
            serde_json::from_slice(&json).expect("Should get analytics response");
        assert_eq!(fetched_analytics.len(), 1);
        assert!(fetched_analytics.get(0).unwrap().payout_count.is_some());
        assert_eq!(fetched_analytics.get(0).unwrap().payout_count.unwrap(), 4);

        // Test with start date
        let start_date = DateHour::<Utc>::now() - 1;

        let query = AnalyticsQuery { 
            limit: 1000,
            event_type: "CLICK".into(),
            metric: Metric::Count,
            timeframe: Timeframe::Day,
            segment_by: None,
            start: Some(AnalyticsQueryTime::Date(start_date)),
            end: None,
            campaign_id: None,
            ad_unit: None,
            ad_slot: None,
            ad_slot_type: None,
            advertiser: None,
            publisher: None,
            hostname: None,
            country: None,
            os_name: None
        };
        let query = serde_urlencoded::to_string(query).expect("should parse query");
        let req = Request::builder()
            .extension(query)
            .body(Body::empty())
            .expect("Should build Request");

        let analytics_response = analytics(
            req,
            &app,
            Some(vec!["country".into(), "ad_slot_type".into()]),
            None,
        )
        .await
        .expect("Should get analytics data");
        let json = hyper::body::to_bytes(analytics_response.into_body())
            .await
            .expect("Should get json");

        let fetched_analytics: Vec<FetchedAnalytics> =
            serde_json::from_slice(&json).expect("Should get analytics response");
        assert_eq!(fetched_analytics.len(), 1);
        assert!(fetched_analytics.get(0).unwrap().payout_count.is_some());
        assert_eq!(fetched_analytics.get(0).unwrap().payout_count.unwrap(), 2);

        // Test with end date
        let end_date = DateHour::<Utc>::now() - 1;
        let req = Request::builder()
            .uri(format!("http://127.0.0.1/analytics?limit=100&eventType=CLICK&metric=count&timeframe=day&end={}", end_date))
            .body(Body::empty())
            .expect("Should build Request");

        let analytics_response = analytics(
            req,
            &app,
            Some(vec!["country".into(), "ad_slot_type".into()]),
            None,
        )
        .await
        .expect("Should get analytics data");
        let json = hyper::body::to_bytes(analytics_response.into_body())
            .await
            .expect("Should get json");

        let fetched_analytics: Vec<FetchedAnalytics> =
            serde_json::from_slice(&json).expect("Should get analytics response");
        assert_eq!(fetched_analytics.len(), 1);
        assert!(fetched_analytics.get(0).unwrap().payout_count.is_some());
        assert_eq!(fetched_analytics.get(0).unwrap().payout_count.unwrap(), 3);

        // Test with start_date and end_date
        // subtract 72 hours
        let start_date = DateHour::<Utc>::now() - 72;
        // subtract 1 hour
        let end_date = DateHour::<Utc>::now() - 1;
        let req = Request::builder()
            .uri(format!("http://127.0.0.1/analytics?limit=100&eventType=CLICK&metric=count&timeframe=day&start={}&end={}", start_date, end_date))
            .body(Body::empty())
            .expect("Should build Request");

        let analytics_response = analytics(
            req,
            &app,
            Some(vec!["country".into(), "ad_slot_type".into()]),
            None,
        )
        .await
        .expect("Should get analytics data");
        let json = hyper::body::to_bytes(analytics_response.into_body())
            .await
            .expect("Should get json");

        let fetched_analytics: Vec<FetchedAnalytics> =
            serde_json::from_slice(&json).expect("Should get analytics response");
        assert_eq!(fetched_analytics.len(), 1);
        assert!(fetched_analytics.get(0).unwrap().payout_count.is_some());
        assert_eq!(fetched_analytics.get(0).unwrap().payout_count.unwrap(), 2);

        // Test with segment_by
        let req = Request::builder()
            .uri("http://127.0.0.1/analytics?limit=100&eventType=CLICK&metric=count&timeframe=day&segment_by=country&country=Bulgaria")
            .body(Body::empty())
            .expect("Should build Request");

        let analytics_response = analytics(
            req,
            &app,
            Some(vec!["country".into(), "ad_slot_type".into()]),
            None,
        )
        .await
        .expect("Should get analytics data");
        let json = hyper::body::to_bytes(analytics_response.into_body())
            .await
            .expect("Should get json");

        let fetched_analytics: Vec<FetchedAnalytics> =
            serde_json::from_slice(&json).expect("Should get analytics response");
        assert_eq!(fetched_analytics.len(), 1);
        assert!(fetched_analytics.get(0).unwrap().payout_count.is_some());
        assert_eq!(fetched_analytics.get(0).unwrap().payout_count.unwrap(), 4);

        // Test with not allowed segment by
        let req = Request::builder()
            .uri("http://127.0.0.1/analytics?limit=100&eventType=CLICK&metric=count&timeframe=day&segment_by=campaignId&campaignId=0x936da01f9abd4d9d80c702af85c822a8")
            .body(Body::empty())
            .expect("Should build Request");

        let analytics_response = analytics(
            req,
            &app,
            Some(vec!["country".into(), "ad_slot_type".into()]),
            None,
        )
        .await;

        let err_msg = "Disallowed segmentBy: campaignId".to_string();
        assert!(matches!(
            analytics_response,
            Err(ResponseError::BadRequest(err_msg))
        ));

        // test with not allowed key
        let req = Request::builder()
            .uri("http://127.0.0.1/analytics?limit=100&eventType=CLICK&metric=count&timeframe=day&campaignId=0x936da01f9abd4d9d80c702af85c822a8")
            .body(Body::empty())
            .expect("Should build Request");

        let analytics_response = analytics(
            req,
            &app,
            Some(vec!["country".into(), "ad_slot_type".into()]),
            None,
        )
        .await;

        let err_msg = "disallowed key in query: campaignId".to_string();
        assert!(matches!(
            analytics_response,
            Err(ResponseError::BadRequest(err_msg))
        ));

        // test with segmentBy which is then not provided
        let req = Request::builder()
            .uri("http://127.0.0.1/analytics?limit=100&eventType=CLICK&metric=count&timeframe=day&segmentBy=country")
            .body(Body::empty())
            .expect("Should build Request");

        let analytics_response = analytics(
            req,
            &app,
            Some(vec!["country".into(), "ad_slot_type".into()]),
            None,
        )
        .await;

        let err_msg = "SegmentBy is provided but a key is not passed".to_string();
        assert!(matches!(
            analytics_response,
            Err(ResponseError::BadRequest(err_msg))
        ));

        // test with different metric
        let req = Request::builder()
            .uri("http://127.0.0.1/analytics?limit=100&eventType=CLICK&metric=paid&timeframe=day")
            .body(Body::empty())
            .expect("Should build Request");

        let analytics_response = analytics(
            req,
            &app,
            Some(vec!["country".into(), "ad_slot_type".into()]),
            None,
        )
        .await
        .expect("Should get analytics data");
        let json = hyper::body::to_bytes(analytics_response.into_body())
            .await
            .expect("Should get json");

        let fetched_analytics: Vec<FetchedAnalytics> =
            serde_json::from_slice(&json).expect("Should get analytics response");
        assert_eq!(fetched_analytics.len(), 1);
        assert!(fetched_analytics.get(0).unwrap().payout_amount.is_some());
        assert_eq!(
            fetched_analytics.get(0).unwrap().payout_amount.unwrap(),
            UnifiedNum::from_u64(4_000_000)
        );

        // Test with different timeframe
        let req = Request::builder()
            .uri("http://127.0.0.1/analytics?limit=100&eventType=CLICK&metric=count&timeframe=week")
            .body(Body::empty())
            .expect("Should build Request");

        let analytics_response = analytics(
            req,
            &app,
            Some(vec!["country".into(), "ad_slot_type".into()]),
            None,
        )
        .await
        .expect("Should get analytics data");
        let json = hyper::body::to_bytes(analytics_response.into_body())
            .await
            .expect("Should get json");

        let fetched_analytics: Vec<FetchedAnalytics> =
            serde_json::from_slice(&json).expect("Should get analytics response");
        assert_eq!(fetched_analytics.len(), 1);
        assert!(fetched_analytics.get(0).unwrap().payout_count.is_some());
        assert_eq!(fetched_analytics.get(0).unwrap().payout_count.unwrap(), 5);

        // Test with a limit
        let req = Request::builder()
            .uri("http://127.0.0.1/analytics?limit=2&eventType=CLICK&metric=count&timeframe=day")
            .body(Body::empty())
            .expect("Should build Request");

        let analytics_response = analytics(
            req,
            &app,
            Some(vec!["country".into(), "ad_slot_type".into()]),
            None,
        )
        .await
        .expect("Should get analytics data");
        let json = hyper::body::to_bytes(analytics_response.into_body())
            .await
            .expect("Should get json");

        let fetched_analytics: Vec<FetchedAnalytics> =
            serde_json::from_slice(&json).expect("Should get analytics response");
        assert_eq!(fetched_analytics.len(), 1);
        assert!(fetched_analytics.get(0).unwrap().payout_count.is_some());
        assert_eq!(fetched_analytics.get(0).unwrap().payout_count.unwrap(), 2);
        // Test with a month timeframe
        let req = Request::builder()
            .uri(
                "http://127.0.0.1/analytics?limit=100&eventType=CLICK&metric=count&timeframe=month",
            )
            .body(Body::empty())
            .expect("Should build Request");

        let analytics_response = analytics(
            req,
            &app,
            Some(vec!["country".into(), "ad_slot_type".into()]),
            None,
        )
        .await
        .expect("Should get analytics data");
        let json = hyper::body::to_bytes(analytics_response.into_body())
            .await
            .expect("Should get json");

        let fetched_analytics: Vec<FetchedAnalytics> =
            serde_json::from_slice(&json).expect("Should get analytics response");
        assert_eq!(fetched_analytics.len(), 1);
        assert!(fetched_analytics.get(0).unwrap().payout_count.is_some());
        assert_eq!(fetched_analytics.get(0).unwrap().payout_count.unwrap(), 6);
        // Test with a year timeframe
        let req = Request::builder()
            .uri("http://127.0.0.1/analytics?limit=100&eventType=CLICK&metric=count&timeframe=year")
            .body(Body::empty())
            .expect("Should build Request");

        let analytics_response = analytics(
            req,
            &app,
            Some(vec!["country".into(), "ad_slot_type".into()]),
            None,
        )
        .await
        .expect("Should get analytics data");
        let json = hyper::body::to_bytes(analytics_response.into_body())
            .await
            .expect("Should get json");

        let fetched_analytics: Vec<FetchedAnalytics> =
            serde_json::from_slice(&json).expect("Should get analytics response");
        assert_eq!(fetched_analytics.len(), 1);
        assert!(fetched_analytics.get(0).unwrap().payout_count.is_some());
        assert_eq!(fetched_analytics.get(0).unwrap().payout_count.unwrap(), 7);

        // Test with start and end as timestamps
    }

    async fn insert_mock_analytics_for_auth_routes(pool: &DbPool) {
        // Analytics with publisher and advertiser
        let now_date = DateHour::try_from(Utc::today().and_hms(1, 0, 0)).expect("should parse");
        let analytics_now = UpdateAnalytics {
            time: now_date,
            campaign_id: DUMMY_CAMPAIGN.id,
            ad_unit: Some(DUMMY_IPFS[0]),
            ad_slot: Some(DUMMY_IPFS[1]),
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
        update_analytics(pool, analytics_now)
            .await
            .expect("Should update analytics");
        // Analytics with a different unit/slot
        let analytics_now = UpdateAnalytics {
            time: now_date,
            campaign_id: DUMMY_CAMPAIGN.id,
            ad_unit: Some(DUMMY_IPFS[2]),
            ad_slot: Some(DUMMY_IPFS[3]),
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
        update_analytics(pool, analytics_now)
            .await
            .expect("Should update analytics");
        // Analytics with a different event type
        let analytics_now = UpdateAnalytics {
            time: now_date,
            campaign_id: DUMMY_CAMPAIGN.id,
            ad_unit: Some(DUMMY_IPFS[0]),
            ad_slot: Some(DUMMY_IPFS[1]),
            ad_slot_type: None,
            advertiser: ADDRESSES["creator"],
            publisher: ADDRESSES["publisher"],
            hostname: None,
            country: Some("Bulgaria".to_string()),
            os_name: OperatingSystem::map_os("Windows"),
            event_type: "IMPRESSION".to_string(),
            amount_to_add: UnifiedNum::from_u64(1_000_000),
            count_to_add: 1,
        };
        update_analytics(pool, analytics_now)
            .await
            .expect("Should update analytics");
        // Analytics with no None fields
        let analytics_now = UpdateAnalytics {
            time: now_date - 2,
            campaign_id: DUMMY_CAMPAIGN.id,
            ad_unit: Some(DUMMY_IPFS[0]),
            ad_slot: Some(DUMMY_IPFS[1]),
            ad_slot_type: Some("TEST_TYPE".to_string()),
            advertiser: ADDRESSES["creator"],
            publisher: ADDRESSES["publisher"],
            hostname: Some("localhost".to_string()),
            country: Some("Bulgaria".to_string()),
            os_name: OperatingSystem::map_os("Windows"),
            event_type: "CLICK".to_string(),
            amount_to_add: UnifiedNum::from_u64(1_000_000),
            count_to_add: 1,
        };
        update_analytics(pool, analytics_now)
            .await
            .expect("Should update analytics");
        // Analytics with different publisher
        let analytics_now = UpdateAnalytics {
            time: now_date,
            campaign_id: DUMMY_CAMPAIGN.id,
            ad_unit: Some(DUMMY_IPFS[0]),
            ad_slot: Some(DUMMY_IPFS[1]),
            ad_slot_type: None,
            advertiser: ADDRESSES["creator"],
            publisher: ADDRESSES["publisher2"],
            hostname: None,
            country: Some("Bulgaria".to_string()),
            os_name: OperatingSystem::map_os("Windows"),
            event_type: "CLICK".to_string(),
            amount_to_add: UnifiedNum::from_u64(1_000_000),
            count_to_add: 1,
        };
        update_analytics(pool, analytics_now)
            .await
            .expect("Should update analytics");
        // Analytics with different advertiser
        let analytics_now = UpdateAnalytics {
            time: now_date,
            campaign_id: DUMMY_CAMPAIGN.id,
            ad_unit: Some(DUMMY_IPFS[0]),
            ad_slot: Some(DUMMY_IPFS[1]),
            ad_slot_type: None,
            advertiser: ADDRESSES["tester"],
            publisher: ADDRESSES["publisher"],
            hostname: None,
            country: Some("Bulgaria".to_string()),
            os_name: OperatingSystem::map_os("Windows"),
            event_type: "CLICK".to_string(),
            amount_to_add: UnifiedNum::from_u64(1_000_000),
            count_to_add: 1,
        };
        update_analytics(pool, analytics_now)
            .await
            .expect("Should update analytics");
        // Analytics with both a different publisher and advertiser
        let analytics_now = UpdateAnalytics {
            time: now_date,
            campaign_id: DUMMY_CAMPAIGN.id,
            ad_unit: Some(DUMMY_IPFS[0]),
            ad_slot: Some(DUMMY_IPFS[1]),
            ad_slot_type: None,
            advertiser: ADDRESSES["tester"],
            publisher: ADDRESSES["publisher2"],
            hostname: None,
            country: Some("Bulgaria".to_string()),
            os_name: OperatingSystem::map_os("Windows"),
            event_type: "CLICK".to_string(),
            amount_to_add: UnifiedNum::from_u64(1_000_000),
            count_to_add: 1,
        };
        update_analytics(pool, analytics_now)
            .await
            .expect("Should update analytics");
    }
    #[tokio::test]
    async fn test_analytics_route_with_auth() {
        let app = setup_dummy_app().await;
        insert_mock_analytics_for_auth_routes(&app.pool).await;

        let publisher_auth = Auth {
            era: 0,
            uid: ValidatorId::from(ADDRESSES["publisher"]),
        };
        let advertiser_auth = Auth {
            era: 0,
            uid: ValidatorId::from(ADDRESSES["advertiser"]),
        };
        let admin_auth = Auth {
            era: 0,
            uid: ValidatorId::try_from("0xce07CbB7e054514D590a0262C93070D838bFBA2e")
                .expect("should create"),
        };
        // test for publisher
        let req = Request::builder()
            .extension(publisher_auth.clone())
            .uri("http://127.0.0.1/analytics?limit=100&eventType=CLICK&metric=count&timeframe=day")
            .body(Body::empty())
            .expect("Should build Request");

        let analytics_response = analytics(req, &app, None, Some("publisher".to_string()))
            .await
            .expect("Should get analytics data");
        let json = hyper::body::to_bytes(analytics_response.into_body())
            .await
            .expect("Should get json");

        let fetched_analytics: Vec<FetchedAnalytics> =
            serde_json::from_slice(&json).expect("Should get analytics response");
        assert_eq!(fetched_analytics.len(), 1);
        assert!(fetched_analytics.get(0).unwrap().payout_count.is_some());
        assert_eq!(fetched_analytics.get(0).unwrap().payout_count.unwrap(), 5);
        // test for advertiser
        let req = Request::builder()
            .extension(advertiser_auth)
            .uri("http://127.0.0.1/analytics?limit=100&eventType=CLICK&metric=count&timeframe=day")
            .body(Body::empty())
            .expect("Should build Request");

        let analytics_response = analytics(req, &app, None, Some("advertiser".to_string()))
            .await
            .expect("Should get analytics data");
        let json = hyper::body::to_bytes(analytics_response.into_body())
            .await
            .expect("Should get json");

        let fetched_analytics: Vec<FetchedAnalytics> =
            serde_json::from_slice(&json).expect("Should get analytics response");
        assert_eq!(fetched_analytics.len(), 1);
        assert!(fetched_analytics.get(0).unwrap().payout_count.is_some());
        assert_eq!(fetched_analytics.get(0).unwrap().payout_count.unwrap(), 5);
        // test for admin
        let req = Request::builder()
            .extension(admin_auth.clone())
            .uri("http://127.0.0.1/analytics?limit=100&eventType=CLICK&metric=count&timeframe=day")
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
        assert_eq!(fetched_analytics.len(), 1);
        assert!(fetched_analytics.get(0).unwrap().payout_count.is_some());
        assert_eq!(fetched_analytics.get(0).unwrap().payout_count.unwrap(), 7);
        // test for admin with all optional keys
        let start_date = DateHour::<Utc>::now() - 72;
        let end_date = DateHour::<Utc>::now() - 1;
        let req = Request::builder()
            .extension(admin_auth)
            .uri(format!("http://127.0.0.1/analytics?limit=100&eventType=CLICK&metric=count&timeframe=day&segmentBy=campaignId&start={}&end={}&campaignId={}&adUnit={}&adSlot={}&adSlotType=TEST_TYPE&advertiser={}&publisher={}&hostname=localhost&country=Bulgaria&osName=Windows", start_date, end_date, DUMMY_CAMPAIGN.id, DUMMY_IPFS[0], DUMMY_IPFS[1], ADDRESSES["creator"], ADDRESSES["publisher"]))
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
        assert_eq!(fetched_analytics.len(), 1);
        assert!(fetched_analytics.get(0).unwrap().payout_count.is_some());
        assert_eq!(fetched_analytics.get(0).unwrap().payout_count.unwrap(), 1);
        // test with no authUid
        let req = Request::builder()
            .uri("http://127.0.0.1/analytics?limit=100&eventType=CLICK&metric=count&timeframe=day")
            .body(Body::empty())
            .expect("Should build Request");

        let analytics_response = analytics(req, &app, None, Some("publisher".to_string())).await;
        let err_msg = "auth_as_key is provided but there is no Auth object".to_string();
        assert!(matches!(
            analytics_response,
            Err(ResponseError::BadRequest(err_msg))
        ));
    }
}
