use crate::{db::analytics::get_analytics, success_response, Application, Auth, ResponseError};
use chrono::{Duration, Utc};
use hyper::{Body, Request, Response};
use once_cell::sync::Lazy;
use primitives::{
    adapter::Adapter,
    analytics::{AnalyticsQuery, Metric, ANALYTICS_QUERY_LIMIT},
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

    let period_in_hours = query.timeframe.get_period_in_hours();
    let start_date = match query.start {
        Some(start_date) => DateHour::try_from(start_date)?,
        None => DateHour::try_from(Utc::now() - Duration::hours(period_in_hours))?,
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

    for key in query.keys() {
        if !allowed_keys.contains(&key) {
            return Err(ResponseError::BadRequest(format!(
                "disallowed key in query: {}",
                key
            )));
        }
    }

    let auth = req
        .extensions()
        .get::<Auth>()
        .expect("request should have session")
        .to_owned();

    let analytics = get_analytics(
        &app.pool,
        start_date,
        end_date,
        &query,
        auth_as_key,
        auth.uid,
        applied_limit,
    )
    .await?;

    let mut count = 0;
    let paid = UnifiedNum::from_u64(0);

    // TODO: Discuss this part and potentially implement it as logic in the SQL Query
    let output: FetchedAnalytics = match &query.metric {
        Metric::Count => {
            analytics.iter().for_each(|entry| {
                count += entry
                    .payout_count
                    .expect("payout_count should be selected and not null")
            });
            FetchedAnalytics {
                payout_count: Some(count),
                payout_amount: None,
            }
        }
        Metric::Paid => {
            analytics.iter().for_each(|entry| {
                paid.checked_add(
                    &entry
                        .payout_amount
                        .expect("payout_amount should be selected and not null"),
                )
                .expect("TODO");
            });
            FetchedAnalytics {
                payout_count: None,
                payout_amount: Some(paid),
            }
        }
    };

    Ok(success_response(serde_json::to_string(&output)?))
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
