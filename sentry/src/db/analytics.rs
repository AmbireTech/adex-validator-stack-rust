use hyper::{Body, Request, Response};
use crate::db::DbPool;
use bb8::RunError;
use primitives::{Channel, ChannelId, ValidatorId};
use primitives::analytics::{AnalyticsResponse, AnalyticsQuery};
use crate::RouteParams;
use crate::Session;
use crate::ResponseError;
use chrono::Utc;

pub async fn get_analytics(
    query: AnalyticsQuery,
    route_params: Option<&RouteParams>,
    sess: Option<&Session>,
    pool: &DbPool,
    is_advertiser: bool,
    skip_publisher_filter: bool,
) -> Result<Vec<AnalyticsResponse>, ResponseError> {
    let applied_limit = query.limit.min(200);
    let (interval, period) = get_time_frame(&query.timeframe);
    let time_limit = Utc::now().timestamp() - period;

    let mut where_clauses = vec![format!("created > to_timestamp({})", time_limit)];

    if is_advertiser {
        match route_params {
            Some(params) => where_clauses.push(format!("channel_id IN ({})", params.index(0))),
            None => where_clauses.push(format!(
                "channel_id IN (SELECT id FROM channels WHERE creator = {})",
                sess.unwrap().uid.to_string()
            )),
        };
    } else if let Some(params) = route_params {
        if let Some(id) = params.get(0) {
            where_clauses.push(format!("channel_id = {}", id));
        };
    }

    let select_query = match (skip_publisher_filter, sess) {
        (false, Some(session)) => {
            where_clauses.push(format!(
                "events->'{}'->'{}'->'{}' IS NOT NULL",
                query.event_type, query.metric, session.uid
            ));
            format!(
                "select SUM((events->'{}'->'{}'->>'{}')::numeric) as value, (extract(epoch from created) - (MOD( CAST (extract(epoch from created) AS NUMERIC), {}))) as time from event_aggregates", 
                query.event_type, query.metric, session.uid, interval
            )
        }
        _ => {
            where_clauses.push(format!(
                "events->'{}'->'{}' IS NOT NULL",
                query.event_type, query.metric
            ));
            format!(
                "select SUM(value::numeric)::varchar as value, (extract(epoch from created) - (MOD( CAST (extract(epoch from created) AS NUMERIC), {}))) as time from event_aggregates, jsonb_each_text(events->'{}'->'{}')", 
                interval, query.event_type, query.metric
            )
        }
    };

    let sql_query = format!(
        "{} WHERE {} GROUP BY time LIMIT {}",
        select_query,
        where_clauses.join(" AND "),
        applied_limit
    );

    // execute query
    pool
        .run(move |connection| {
            async move {
                match connection.prepare(&sql_query).await {
                    Ok(stmt) => match connection.query(&stmt, &[]).await {
                        Ok(rows) => {
                            let analytics: Vec<AnalyticsResponse> =
                                rows.iter().map(AnalyticsResponse::from).collect();
                            Ok((analytics, connection))
                        }
                        Err(e) => Err((e, connection)),
                    },
                    Err(e) => Err((e, connection)),
                }
            }
        })
        .await;
}


fn get_time_frame(timeframe: &str) -> (i64, i64) {
    let minute = 60 * 1000;
    let hour = 60 * minute;
    let day = 24 * hour;

    match timeframe {
        "year" => (30 * day, 365 * day),
        "month" => (day, 30 * day),
        "week" => (6 * hour, 7 * day),
        "day" => (hour, day),
        "hour" => (minute, hour),
        _ => (hour, day),
    }
}
