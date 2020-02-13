use crate::db::DbPool;
use crate::Session;
use bb8::RunError;
use chrono::Utc;
use primitives::analytics::{AnalyticsQuery, AnalyticsResponse, ANALYTICS_QUERY_LIMIT};
use primitives::{ValidatorId, ChannelId};
use primitives::sentry::{Event, AdvancedAnalyticsResponse};
use redis::aio::MultiplexedConnection;
use crate::{ epoch };
use redis::AsyncCommands;
use redis::Commands;
use redis;
use std::collections::HashMap;
use std::error::Error;
use futures::future::try_join_all;
use futures::future::TryFutureExt;


pub enum AnalyticsType {
    Advertiser {
        session: Session,
        channel: Option<String>,
    },
    Global,
    Publisher {
        session: Session,
        channel: Option<String>,
    },
}

pub async fn get_analytics(
    query: AnalyticsQuery,
    pool: &DbPool,
    analytics_type: AnalyticsType,
) -> Result<Vec<AnalyticsResponse>, RunError<bb8_postgres::tokio_postgres::Error>> {
    let applied_limit = query.limit.min(ANALYTICS_QUERY_LIMIT);
    let (interval, period) = get_time_frame(&query.timeframe);
    let time_limit = Utc::now().timestamp() - period;

    let mut where_clauses = vec![format!("created > to_timestamp({})", time_limit)];

    let select_query = match analytics_type {
        AnalyticsType::Advertiser { session, channel } => {
            if let Some(id) = channel {
                where_clauses.push(format!("channel_id = {}", id));
            } else {
                where_clauses.push(format!(
                    "channel_id IN (SELECT id FROM channels WHERE creator = {})",
                    session.uid
                ));
            }

            where_clauses.push(format!(
                "events->'{}'->'{}' IS NOT NULL",
                query.event_type, query.metric
            ));

            format!(
                "select SUM(value::numeric)::varchar as value, (extract(epoch from created) - (MOD( CAST (extract(epoch from created) AS NUMERIC), {}))) as time from event_aggregates, jsonb_each_text(events->'{}'->'{}')", 
                interval, query.event_type, query.metric
            )
        }
        AnalyticsType::Global => {
            where_clauses.push(format!(
                "events->'{}'->'{}' IS NOT NULL",
                query.event_type, query.metric
            ));
            format!(
                "select SUM(value::numeric)::varchar as value, (extract(epoch from created) - (MOD( CAST (extract(epoch from created) AS NUMERIC), {}))) as time from event_aggregates, jsonb_each_text(events->'{}'->'{}')", 
                interval, query.event_type, query.metric
            )
        }
        AnalyticsType::Publisher { session, channel } => {
            if let Some(id) = channel {
                where_clauses.push(format!("channel_id = {}", id));
            }

            where_clauses.push(format!(
                "events->'{}'->'{}'->'{}' IS NOT NULL",
                query.event_type, query.metric, session.uid
            ));

            format!(
                "select SUM((events->'{}'->'{}'->>'{}')::numeric) as value, (extract(epoch from created) - (MOD( CAST (extract(epoch from created) AS NUMERIC), {}))) as time from event_aggregates", 
                query.event_type, query.metric, session.uid, interval
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
    pool.run(move |connection| async move {
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
    })
    .await
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

async fn stat_pair(conn: MultiplexedConnection, key: &str ) -> Result<HashMap<String, String>, Box<dyn Error>> {
    let data = redis::cmd("ZRANGE")
                    .arg(key)
                    .arg(0 as u64)
                    .arg(-1 as i64)
                    .arg("WITHSCORES")
                    .query_async::<_, Vec<String>>(&mut conn.clone())
                    .await?;

    let stats = data.chunks(2).map(|&chunk|(chunk[0], chunk[1]));
    let result: HashMap<String, String> = stats.into_iter().collect();

    Ok(result)
}

pub async fn get_advanced_reports(redis: &MultiplexedConnection,  event: &Event, publisher: &ValidatorId, channelIds: &[ChannelId]) -> Result<AdvancedAnalyticsResponse, Box<dyn Error>> {
    let publisher_keys = [
        format!("reportPublisherToAdUnit:{}:{}", event, publisher),
        format!("reportPublisherToAdSlot:{}:{}", event, publisher),
        format!("reportPublisherToAdSlotPay:{}:{}", event, publisher),
        format!("reportPublisherToCountry:{}:{}", event, publisher),
        format!("reportPublisherToHostname:{}:{}", event, publisher),
    ];

    let publisher_stats = HashMap::new();

    for publisher_key in publisher_keys.iter()  {
        let result  = stat_pair(redis.clone(), &publisher_key).await?;
        publisher_stats.insert(
            publisher_key.split(":").nth(0).expect("should always have key").to_string(), 
            result
        );
    }

    let by_channel_stats = HashMap::new();

    for channelId in channelIds {
        let keys = [
            format!("reportChannelToAdUnit:{}:{}", event, channelId),
            format!("reportChannelToHostname:{}:{}", event, channelId),
            format!("reportChannelToHostnamePay:{}:{}", event, channelId),
        ];
        
        let channel_stat = HashMap::new();

        for key in keys.iter()  {
            let result = stat_pair(redis.clone(), &key).await?;
            channel_stat.insert(
                key.split(":").nth(0).expect("should always have key").to_string(),
                result
            );
        }

        by_channel_stats.insert(channelId.to_owned(), channel_stat);
    }

    Ok(AdvancedAnalyticsResponse {
        publisher_stats,
        by_channel_stats
    })
}