use chrono::{DateTime, Utc};
use primitives::{
    analytics::{AnalyticsQuery, AnalyticsQueryTime, AuthenticateAs, Metric},
    sentry::{Analytics, FetchedAnalytics, UpdateAnalytics},
};
use tokio_postgres::types::ToSql;

use super::{DbPool, PoolError};

pub async fn get_analytics(
    pool: &DbPool,
    start_date: &AnalyticsQueryTime,
    query: &AnalyticsQuery,
    allowed_keys: Vec<&str>,
    auth_as: Option<AuthenticateAs>,
    limit: u32,
) -> Result<Vec<FetchedAnalytics>, PoolError> {
    let client = pool.get().await?;

    let (where_clauses, params) =
        analytics_query_params(start_date, query, &auth_as, &allowed_keys);

    let mut select_clause = vec!["time".to_string()];
    match &query.metric {
        Metric::Paid => select_clause.push("payout_amount".to_string()),
        Metric::Count => select_clause.push("payout_count".to_string()),
    }
    let mut group_clause = vec!["time".to_string()];

    if let Some(segment_by) = &query.segment_by {
        select_clause.push(segment_by.to_string());
        group_clause.push(segment_by.to_string());
    }

    // TODO: Is a GROUP BY clause really needed here as we call merge_analytics() later and get the same output
    let sql_query = format!(
        "SELECT {} FROM analytics WHERE {} ORDER BY time ASC LIMIT {}",
        select_clause.join(", "),
        where_clauses.join(" AND "),
        // group_clause.join(", "),
        limit,
    );

    // execute query
    let stmt = client.prepare(&sql_query).await?;
    let rows = client.query(&stmt, params.as_slice()).await?;

    let analytics: Vec<FetchedAnalytics> = rows
        .iter()
        .map(|row| {
            // Since segment_by is a dynamic value/type it can't be passed to from<&Row> so we're building the object here
            let segment_value = match &query.segment_by {
                Some(segment_by) => row.try_get(&**segment_by).ok(),
                None => None,
            };
            let time = row.get::<_, DateTime<Utc>>("time");

            FetchedAnalytics {
                time: time.timestamp(),
                payout_amount: row.try_get("payout_amount").ok(),
                payout_count: row.try_get("payout_count").ok(),
                segment: segment_value,
            }
        })
        .collect();

    Ok(analytics)
}

fn analytics_query_params<'a>(
    start_date: &'a AnalyticsQueryTime,
    query: &'a AnalyticsQuery,
    auth_as: &'a Option<AuthenticateAs>,
    allowed_keys: &[&str],
) -> (Vec<String>, Vec<&'a (dyn ToSql + Sync)>) {
    let mut where_clauses: Vec<String> = vec!["time >= $1".to_string()];
    let mut params: Vec<&(dyn ToSql + Sync)> = vec![start_date];

    allowed_keys.iter().for_each(|key| {
        if let Some(param_value) = query.get_key(key) {
            where_clauses.push(format!("{} = ${}", key, params.len() + 1));
            params.push(param_value);
        }
    });

    if let Some(auth_as) = auth_as {
        match auth_as {
            AuthenticateAs::Publisher(uid) => {
                where_clauses.push(format!("publisher = ${}", params.len() + 1));
                params.push(uid);
            }
            AuthenticateAs::Advertiser(uid) => {
                where_clauses.push(format!("advertiser = ${}", params.len() + 1));
                params.push(uid);
            }
        }
    }

    if let Some(end_date) = &query.end {
        where_clauses.push(format!("time <= ${}", params.len() + 1));
        params.push(end_date);
    }
    where_clauses.push(format!("event_type = ${}", params.len() + 1));
    params.push(&query.event_type);

    where_clauses.push(format!("{} IS NOT NULL", query.metric.column_name()));

    (where_clauses, params)
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
