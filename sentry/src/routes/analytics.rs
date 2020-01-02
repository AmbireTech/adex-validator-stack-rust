use crate::db::analytics::get_analytics;
use crate::success_response;
use crate::Application;
use crate::ResponseError;
use crate::RouteParams;
use crate::Session;
use hyper::{Body, Request, Response};
use primitives::adapter::Adapter;
use primitives::analytics::AnalyticsQuery;
use redis::aio::MultiplexedConnection;
use slog::{error, Logger};

pub async fn publisher_analytics<A: Adapter>(
    req: Request<Body>,
    app: &Application<A>,
) -> Result<Response<Body>, ResponseError> {
    process_analytics(req, app, false, true)
        .await
        .map(success_response)
}

pub async fn analytics<A: Adapter>(
    req: Request<Body>,
    app: &Application<A>,
) -> Result<Response<Body>, ResponseError> {
    let request_uri = req.uri().to_string();
    let redis = app.redis.clone();

    match redis::cmd("GET")
        .arg(&request_uri)
        .query_async::<_, Option<String>>(&mut redis.clone())
        .await
    {
        Ok(Some(response)) => Ok(success_response(response)),
        _ => {
            // checks if /:id route param is present
            let cache_timeframe = match req.extensions().get::<RouteParams>() {
                Some(_) => 600,
                None => 300,
            };
            let response = process_analytics(req, app, false, false).await?;
            cache(&redis.clone(), request_uri, &response, cache_timeframe, &app.logger).await;
            Ok(success_response(response))
        }
    }
}

pub async fn advertiser_analytics<A: Adapter>(
    req: Request<Body>,
    app: &Application<A>,
) -> Result<Response<Body>, ResponseError> {
    process_analytics(req, app, true, false)
        .await
        .map(success_response)
}

pub async fn process_analytics<A: Adapter>(
    req: Request<Body>,
    app: &Application<A>,
    is_advertiser: bool,
    filter_publisher: bool,
) -> Result<String, ResponseError> {
    let query = serde_urlencoded::from_str::<AnalyticsQuery>(&req.uri().query().unwrap_or(""))?;
    query
        .is_valid()
        .map_err(|e| ResponseError::BadRequest(e.to_string()))?;

    let sess = req.extensions().get::<Session>();
    let channels = match req.extensions().get::<RouteParams>() {
        Some(param) => param.get(0),
        None => None,
    };

    let result = get_analytics(
        query,
        channels,
        sess,
        &app.pool,
        is_advertiser,
        filter_publisher,
    )
    .await?;

    serde_json::to_string(&result)
        .map_err(|_| ResponseError::BadRequest("error occurred; try again later".to_string()))
}

async fn cache(redis: &MultiplexedConnection, key: String, value: &str, timeframe: i32, logger: &Logger) {
    if let Err(err) = redis::cmd("SETEX")
        .arg(&key)
        .arg(timeframe)
        .arg(value)
        .query_async::<_, ()>(&mut redis.clone())
        .await
    {
        error!(&logger, "server error: {}", err);
    }
}
