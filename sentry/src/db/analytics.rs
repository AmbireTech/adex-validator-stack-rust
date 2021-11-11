use crate::{epoch, Auth};
use chrono::Utc;
use primitives::{
    analytics::{AnalyticsData, AnalyticsQuery, ANALYTICS_QUERY_LIMIT},
    sentry::{
        AdvancedAnalyticsResponse, Analytics, ChannelReport, PublisherReport, UpdateAnalytics,
    },
    ChannelId, ValidatorId,
};
use redis::{aio::MultiplexedConnection, cmd};
use std::collections::HashMap;
use tokio_postgres::types::ToSql;

use super::{DbPool, PoolError};

pub enum AnalyticsType {
    Advertiser { auth: Auth },
    Global,
    Publisher { auth: Auth },
}

pub async fn advertiser_channel_ids(
    pool: &DbPool,
    creator: &ValidatorId,
) -> Result<Vec<ChannelId>, PoolError> {
    let client = pool.get().await?;

    let stmt = client
        .prepare("SELECT id FROM channels WHERE creator = $1")
        .await?;
    let rows = client.query(&stmt, &[creator]).await?;

    let channel_ids: Vec<ChannelId> = rows.iter().map(ChannelId::from).collect();
    Ok(channel_ids)
}

fn metric_to_column(metric: &str) -> String {
    match metric {
        "eventCounts" => "count".to_string(),
        "eventPayouts" => "payout".to_string(),
        _ => "count".to_string(),
    }
}

pub async fn get_analytics(
    query: AnalyticsQuery,
    pool: &DbPool,
    analytics_type: AnalyticsType,
    segment_by_channel: bool,
    channel_id: Option<&ChannelId>,
) -> Result<Vec<AnalyticsData>, PoolError> {
    let client = pool.get().await?;

    // converts metric to column
    let metric = metric_to_column(&query.metric);

    let mut params = Vec::<&(dyn ToSql + Sync)>::new();
    let applied_limit = query.limit.min(ANALYTICS_QUERY_LIMIT);
    let (interval, period) = get_time_frame(&query.timeframe);
    let time_limit = Utc::now().timestamp() - period;

    let mut where_clauses = vec![format!("created > to_timestamp({})", time_limit)];

    params.push(&query.event_type);

    where_clauses.extend(vec![
        format!("event_type = ${}", params.len()),
        format!("{} IS NOT NULL", metric),
    ]);

    if let Some(id) = channel_id {
        where_clauses.push(format!("channel_id = '{}'", id));
    }

    let mut group_clause = "time".to_string();
    let mut select_clause = match analytics_type {
        AnalyticsType::Advertiser { auth } => {
            if channel_id.is_none() {
                where_clauses.push(format!(
                    "channel_id IN (SELECT id FROM channels WHERE creator = '{}')",
                    auth.uid
                ));
            }

            format!(
                "SUM({}::numeric)::varchar as value, (extract(epoch from created) - (MOD( CAST (extract(epoch from created) AS NUMERIC), {}))) as time", 
                metric, interval
            )
        }
        AnalyticsType::Global => {
            where_clauses.push("earner IS NULL".to_string());

            format!(
                "SUM({}::numeric)::varchar as value, (extract(epoch from created) - (MOD( CAST (extract(epoch from created) AS NUMERIC), {}))) as time", 
                metric, interval
            )
        }
        AnalyticsType::Publisher { auth } => {
            where_clauses.push(format!("earner = '{}'", auth.uid));

            format!(
                "SUM({}::numeric)::varchar as value, (extract(epoch from created) - (MOD( CAST (extract(epoch from created) AS NUMERIC), {}))) as time", 
                metric, interval
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

    // execute query
    let stmt = client.prepare(&sql_query).await?;
    let rows = client.query(&stmt, &params).await?;

    let analytics: Vec<AnalyticsData> = rows.iter().map(AnalyticsData::from).collect();

    Ok(analytics)
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
    mut conn: MultiplexedConnection,
    key: &str,
) -> Result<HashMap<String, f64>, Box<dyn std::error::Error>> {
    let data = cmd("ZRANGE")
        .arg(key)
        .arg(0_u64)
        .arg(-1_i64)
        .arg("WITHSCORES")
        .query_async::<_, Vec<String>>(&mut conn)
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
) -> Result<AdvancedAnalyticsResponse, Box<dyn std::error::Error>> {
    let publisher_reports = [
        PublisherReport::AdUnit,
        PublisherReport::AdSlot,
        PublisherReport::AdSlotPay,
        PublisherReport::Country,
        PublisherReport::Hostname,
    ];

    let mut publisher_stats: HashMap<PublisherReport, HashMap<String, f64>> = HashMap::new();

    for publisher_report in publisher_reports.iter() {
        let pair = match publisher_report {
            PublisherReport::Country => format!(
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
        ChannelReport::AdUnit,
        ChannelReport::Hostname,
        ChannelReport::HostnamePay,
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
        by_channel_stats,
        publisher_stats,
    })
}

/// This will update a record when it's present by incrementing its payout_amount and payout_count fields
pub async fn update_analytics(
    pool: &DbPool,
    update_analytics: UpdateAnalytics,
) -> Result<Analytics, PoolError> {
    let client = pool.get().await?;

    let query = "INSERT INTO analytics(campaign_id, time, ad_unit, ad_slot, ad_slot_type, advertiser, publisher, hostname, country, os, event_type, payout_amount, payout_count)
    VALUES ($1, date_trunc('hour', cast($2 as timestamp with time zone)), $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
    ON CONFLICT ON CONSTRAINT analytics_pkey DO UPDATE
    SET payout_amount = analytics.payout_amount + $12, payout_count = analytics.payout_count + 1
    RETURNING campaign_id, time, ad_unit, ad_slot, ad_slot_type, advertiser, publisher, hostname, country, os, event_type, payout_amount, payout_count";

    let stmt = client.prepare(query).await?;

    let row = client
        .query_one(
            &stmt,
            &[
                &update_analytics.campaign_id,
                &update_analytics.time,
                &update_analytics
                    .ad_unit
                    .map(|ipfs| ipfs.to_string())
                    .unwrap_or_default(),
                &update_analytics
                    .ad_slot
                    .map(|ipfs| ipfs.to_string())
                    .unwrap_or_default(),
                &update_analytics.ad_slot_type.clone().unwrap_or_default(),
                &update_analytics.advertiser,
                &update_analytics.publisher,
                &update_analytics
                    .hostname
                    .as_ref()
                    .unwrap_or(&"".to_string()),
                &update_analytics.country.as_ref().unwrap_or(&"".to_string()),
                &update_analytics.os_name.to_string(),
                &update_analytics.event_type,
                &update_analytics.amount_to_add,
                &update_analytics.count_to_add,
            ],
        )
        .await?;

    let event_analytics = Analytics::from(&row);

    Ok(event_analytics)
}

#[cfg(test)]
mod test {
    use super::*;
    use primitives::{
        analytics::OperatingSystem,
        sentry::DateHour,
        util::tests::prep_db::{ADDRESSES, DUMMY_AD_UNITS, DUMMY_CAMPAIGN, DUMMY_IPFS},
        UnifiedNum,
    };

    use crate::db::tests_postgres::{setup_test_migrations, DATABASE_POOL};

    #[tokio::test]
    async fn insert_update_and_get_analytics() {
        let database = DATABASE_POOL.get().await.expect("Should get a DB pool");

        let ad_unit = DUMMY_AD_UNITS[0].clone();
        let ad_slot_ipfs = DUMMY_IPFS[0];

        setup_test_migrations(database.pool.clone())
            .await
            .expect("Migrations should succeed");

        // Inserts a new Analytics and later updates it by adding more payout & count
        {
            let update = UpdateAnalytics {
                time: DateHour::from_ymdh(2021, 2, 1, 1),
                campaign_id: DUMMY_CAMPAIGN.id,
                ad_unit: Some(ad_unit.ipfs),
                ad_slot: Some(ad_slot_ipfs),
                ad_slot_type: Some(ad_unit.ad_type.clone()),
                advertiser: ADDRESSES["creator"],
                publisher: ADDRESSES["publisher"],
                hostname: Some("localhost".to_string()),
                country: Some("Bulgaria".to_string()),
                os_name: OperatingSystem::Linux,
                event_type: "IMPRESSION".to_string(),
                amount_to_add: UnifiedNum::from_u64(1_000_000),
                count_to_add: 1,
            };

            let analytics = update_analytics(&database.clone(), update.clone())
                .await
                .expect("Should insert");

            assert_eq!(update.campaign_id, analytics.campaign_id);
            assert_eq!(update.time.date, analytics.time.date);
            assert_eq!(update.time.hour, analytics.time.hour);
            assert_eq!(update.ad_unit, analytics.ad_unit);
            assert_eq!(update.ad_slot, analytics.ad_slot);
            assert_eq!(update.ad_slot_type, analytics.ad_slot_type);
            assert_eq!(update.advertiser, analytics.advertiser);
            assert_eq!(update.publisher, analytics.publisher);
            assert_eq!(update.hostname, analytics.hostname);
            assert_eq!(update.country, analytics.country);
            assert_eq!(update.os_name, analytics.os_name);
            assert_eq!(update.event_type, analytics.event_type);

            assert_eq!(UnifiedNum::from_u64(1_000_000), analytics.payout_amount);
            assert_eq!(1, analytics.payout_count);

            let analytics_updated = update_analytics(&database.clone(), update.clone())
                .await
                .expect("Should update");
            assert_eq!(
                analytics_updated.payout_amount,
                UnifiedNum::from_u64(2_000_000)
            );
            assert_eq!(analytics_updated.payout_count, 2);
        }

        // On empty fields marked as `NOT NULL` it should successfully insert a new analytics
        {
            let analytics_with_empty_fields = UpdateAnalytics {
                time: DateHour::from_ymdh(2021, 2, 1, 1),
                campaign_id: DUMMY_CAMPAIGN.id,
                ad_unit: None,
                ad_slot: None,
                ad_slot_type: None,
                advertiser: ADDRESSES["creator"],
                publisher: ADDRESSES["publisher"],
                hostname: None,
                country: None,
                os_name: OperatingSystem::Linux,
                event_type: "IMPRESSION".to_string(),
                amount_to_add: UnifiedNum::from_u64(1_000_000),
                count_to_add: 1,
            };

            let insert_res =
                update_analytics(&database.clone(), analytics_with_empty_fields.clone())
                    .await
                    .expect("Should insert");

            assert_eq!(insert_res.ad_unit, analytics_with_empty_fields.ad_unit);
            assert_eq!(insert_res.ad_slot, analytics_with_empty_fields.ad_slot);
            assert_eq!(
                insert_res.ad_slot_type,
                analytics_with_empty_fields.ad_slot_type
            );
            assert_eq!(insert_res.hostname, analytics_with_empty_fields.hostname);
            assert_eq!(insert_res.country, analytics_with_empty_fields.country);
        }
    }
}
