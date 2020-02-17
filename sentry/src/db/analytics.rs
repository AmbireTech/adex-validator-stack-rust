use crate::db::DbPool;
use crate::epoch;
use crate::Session;
use bb8::RunError;
use chrono::Utc;
use primitives::analytics::{AnalyticsQuery, AnalyticsData, ANALYTICS_QUERY_LIMIT};
use primitives::sentry::{AdvancedAnalyticsResponse, ChannelReport, PublisherReport};
use primitives::{ChannelId, ValidatorId};
use redis;
use redis::aio::MultiplexedConnection;
use std::collections::HashMap;
use std::error::Error;

pub enum AnalyticsType {
    Advertiser {
        session: Session,
    },
    Global,
    Publisher {
        session: Session,
    },
}

pub async fn advertiser_channel_ids(
    pool: &DbPool,
    creator: &ValidatorId,
) -> Result<Vec<ChannelId>, RunError<bb8_postgres::tokio_postgres::Error>> {
    pool.run(move |connection| async move {
        match connection
            .prepare("SELECT id FROM channels WHERE creator = $1")
            .await
        {
            Ok(stmt) => match connection.query(&stmt, &[creator]).await {
                Ok(rows) => {
                    let channel_ids: Vec<ChannelId> = rows.iter().map(ChannelId::from).collect();
                    Ok((channel_ids, connection))
                }
                Err(e) => Err((e, connection)),
            },
            Err(e) => Err((e, connection)),
        }
    })
    .await
}

pub async fn get_analytics(
    query: AnalyticsQuery,
    pool: &DbPool,
    analytics_type: AnalyticsType,
    segment_by_channel: bool,
    channel_id: Option<&ChannelId>,
) -> Result<Vec<AnalyticsData>, RunError<bb8_postgres::tokio_postgres::Error>> {
    let applied_limit = query.limit.min(ANALYTICS_QUERY_LIMIT);
    let (interval, period) = get_time_frame(&query.timeframe);
    let time_limit = Utc::now().timestamp() - period;

    let mut where_clauses = vec![format!("created > to_timestamp({})", time_limit)];

    if let Some(id) = channel_id {
        where_clauses.push(format!("channel_id = '{}'", id));
    }

    let mut group_clause = "time".to_string();
    let mut select_clause = match analytics_type {
        AnalyticsType::Advertiser { session } => {
            if channel_id.is_none() {
                where_clauses.push(format!(
                    "channel_id IN (SELECT id FROM channels WHERE creator = '{}')",
                    session.uid
                ));
            }

            where_clauses.push(format!(
                "event_type = '{}'",
                query.event_type
            ));

            where_clauses.push(format!(
                "{} IS NOT NULL",
                query.metric
            ));

            format!(
                "SUM({}::numeric)::varchar as value, (extract(epoch from created) - (MOD( CAST (extract(epoch from created) AS NUMERIC), {}))) as time", 
                query.metric, interval
            )
        }
        AnalyticsType::Global  => {
            where_clauses.push(format!(
                "event_type = '{}'",
                query.event_type
            ));

            where_clauses.push(format!(
                "{} IS NOT NULL",
                query.metric
            ));

            where_clauses.push("earner IS NULL".to_string());

            format!(
                "SUM({}::numeric)::varchar as value, (extract(epoch from created) - (MOD( CAST (extract(epoch from created) AS NUMERIC), {}))) as time", 
                query.metric, interval
            )
        }
        AnalyticsType::Publisher { session } => {
            where_clauses.push(format!(
                "event_type = '{}'",
                query.event_type
            ));

            where_clauses.push(format!(
                "{} IS NOT NULL",
                query.metric
            ));

            where_clauses.push(format!(
                "earner = '{}'",
                session.uid
            ));
            
            format!(
                "SUM({}::numeric)::varchar as value, (extract(epoch from created) - (MOD( CAST (extract(epoch from created) AS NUMERIC), {}))) as time", 
                query.metric, interval
            )
        }
    };

    if segment_by_channel {
        select_clause = format!("{}, channel_id", select_clause);
        group_clause = format!("{}, channel_id", group_clause);
    }

    let sql_query = format!(
        "SELECT {} FROM event_aggregates WHERE {} GROUP BY {} LIMIT {}",
        select_clause,
        where_clauses.join(" AND "),
        group_clause,
        applied_limit,
    );

    println!("{}", sql_query);

    // execute query
    pool.run(move |connection| async move {
        match connection.prepare(&sql_query).await {
            Ok(stmt) => match connection.query(&stmt, &[]).await {
                Ok(rows) => {
                    let analytics: Vec<AnalyticsData> =
                        rows.iter().map(AnalyticsData::from).collect();
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

async fn stat_pair(
    conn: MultiplexedConnection,
    key: &str,
) -> Result<HashMap<String, f64>, Box<dyn Error>> {
    let data = redis::cmd("ZRANGE")
        .arg(key)
        .arg(0 as u64)
        .arg(-1 as i64)
        .arg("WITHSCORES")
        .query_async::<_, Vec<String>>(&mut conn.clone())
        .await?;

    Ok(data
        .chunks(2)
        .map(|chunk: &[String]| {
            (
                chunk[0].clone(),
                chunk[1].parse::<f64>().expect("should parse value"),
            )
        })
        .collect())
}

pub async fn get_advanced_reports(
    redis: &MultiplexedConnection,
    event_type: &str,
    publisher: &ValidatorId,
    channel_ids: &[ChannelId],
) -> Result<AdvancedAnalyticsResponse, Box<dyn Error>> {
    println!("get advnaces");
    let publisher_reports = [
        PublisherReport::ReportPublisherToAdUnit,
        PublisherReport::ReportPublisherToAdSlot,
        PublisherReport::ReportPublisherToAdSlotPay,
        PublisherReport::ReportPublisherToCountry,
        PublisherReport::ReportPublisherToHostname,
    ];

    let mut publisher_stats: HashMap<PublisherReport, HashMap<String, f64>> = HashMap::new();

    for publisher_report in publisher_reports.iter() {
        let pair = match publisher_report {
            PublisherReport::ReportPublisherToCountry => format!(
                "{}:{}:{}:{}",
                epoch().floor(),
                publisher_report,
                event_type,
                publisher
            ),
            _ => format!("{}:{}:{}", publisher_report, event_type, publisher),
        };
        let result = stat_pair(redis.clone(), &pair).await?;
        publisher_stats.insert(publisher_report.clone(), result);
    }

    let mut by_channel_stats = HashMap::new();

    let channel_reports = [
        ChannelReport::ReportChannelToAdUnit,
        ChannelReport::ReportChannelToHostname,
        ChannelReport::ReportChannelToHostnamePay,
    ];

    for channel_id in channel_ids {
        let mut channel_stat = HashMap::new();

        for channel_report in channel_reports.iter() {
            let result = stat_pair(
                redis.clone(),
                &format!("{}:{}:{}", channel_report, event_type, channel_id),
            )
            .await?;
            channel_stat.insert(channel_report.clone(), result);
        }

        by_channel_stats.insert(channel_id.to_owned(), channel_stat);
    }
    

    Ok(AdvancedAnalyticsResponse {
        publisher_stats,
        by_channel_stats,
    })
}
