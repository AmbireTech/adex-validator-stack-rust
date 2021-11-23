use crate::{
    db::analytics::{advertiser_channel_ids, get_advanced_reports, get_analytics, AnalyticsType},
    success_response, Application, Auth, ResponseError, RouteParams,
};
use hyper::{Body, Request, Response};
use primitives::{
    adapter::Adapter,
    analytics::{AnalyticsQuery, AnalyticsResponse},
    sentry::DateHour,
    ChannelId,
};
use redis::aio::MultiplexedConnection;
use slog::{error, Logger};

pub const ALLOWED_KEYS: [&'static str; 9] = [
	"campaignId",
	"adUnit",
	"adSlot",
	"adSlotType",
	"advertiser",
	"publisher",
	"hostname",
	"country",
	"osName"
];

// TODO: Convert timeframe to enum and add this as an enum method
pub fn get_period_in_hours(timeframe: String) -> u64 {
    let hour = 1;
    let day = 24 * hour;
    let year = 365 * day;
    if timeframe == "day" {
        day
    } else if timeframe == "week" {
        7 * day
    } else if timeframe == "month" {
        year / 12
    } else if timeframe == "year" {
        year
    } else {
        day
    }
}

pub fn get_time_period_query_clause(start: Option<DateTime<Utc>, end: Option<DateTime<Utc>>, period: u64, event_type: String, metric: String, timezone: String) -> String {
    // start && !Number.isNaN(new Date(start)) ? new Date(start) : new Date(Date.now() - period),
    let start = match start {
        Some(start) => {
            DateHour::from()
        },
        None => DateHour::now() -
    }
}

pub async fn analytics<A: Adapter>(
    req: Request<Body>,
    app: &Application<A>,
    allowed_keys: Option<Vec<String>>,
    auth_as_key: Option<String>,
) -> Result<Response<Body>, ResponseError> {
    let query = serde_urlencoded::from_str::<AnalyticsQuery>(req.uri().query().unwrap_or(""))?;

    let period = get_period_in_hours(query.timeframe);


    todo!();
}


// TODO: remove each of these
pub async fn publisher_analytics<A: Adapter>(
    req: Request<Body>,
    app: &Application<A>,
) -> Result<Response<Body>, ResponseError> {
    todo!();
    // let auth = req
    //     .extensions()
    //     .get::<Auth>()
    //     .ok_or(ResponseError::Unauthorized)?
    //     .clone();

    // let analytics_type = AnalyticsType::Publisher { auth };

    // process_analytics(req, app, analytics_type)
    //     .await
    //     .map(success_response)
}

pub async fn advertiser_analytics<A: Adapter>(
    req: Request<Body>,
    app: &Application<A>,
) -> Result<Response<Body>, ResponseError> {
    todo!();
    // let sess = req.extensions().get::<Auth>();
    // let analytics_type = AnalyticsType::Advertiser {
    //     auth: sess.ok_or(ResponseError::Unauthorized)?.to_owned(),
    // };

    // process_analytics(req, app, analytics_type)
    //     .await
    //     .map(success_response)
}

pub async fn process_analytics<A: Adapter>(
    req: Request<Body>,
    app: &Application<A>,
    analytics_type: AnalyticsType,
) -> Result<String, ResponseError> {
    todo!();
    // let query = serde_urlencoded::from_str::<AnalyticsQuery>(req.uri().query().unwrap_or(""))?;
    // query
    //     .is_valid()
    //     .map_err(|e| ResponseError::BadRequest(e.to_string()))?;

    // let channel_id = req.extensions().get::<ChannelId>();

    // let segment_channel = query.segment_by_channel.is_some();

    // let limit = query.limit;

    // let aggr = get_analytics(
    //     query,
    //     &app.pool,
    //     analytics_type,
    //     segment_channel,
    //     channel_id,
    // )
    // .await?;

    // let response = AnalyticsResponse { aggr, limit };

    // serde_json::to_string(&response)
    //     .map_err(|_| ResponseError::BadRequest("error occurred; try again later".to_string()))
}

pub async fn admin_analytics<A: Adapter>(
    req: Request<Body>,
    app: &Application<A>,
) -> Result<Response<Body>, ResponseError> {
    todo!();
    // let auth = req.extensions().get::<Auth>().expect("auth is required");
    // let advertiser_channels = advertiser_channel_ids(&app.pool, &auth.uid).await?;

    // let query = serde_urlencoded::from_str::<AnalyticsQuery>(req.uri().query().unwrap_or(""))?;

    // let response = get_advanced_reports(
    //     &app.redis,
    //     &query.event_type,
    //     &auth.uid,
    //     &advertiser_channels,
    // )
    // .await
    // .map_err(|_| ResponseError::BadRequest("error occurred; try again later".to_string()))?;

    // Ok(success_response(serde_json::to_string(&response)?))
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
