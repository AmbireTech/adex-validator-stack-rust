use std::collections::HashSet;

use chrono::{DateTime, Utc};
use futures::TryStreamExt;
use primitives::{
    analytics::{query::AllowedKey, AnalyticsQuery, AuthenticateAs, Metric, Timeframe},
    sentry::{Analytics, FetchedAnalytics, UpdateAnalytics},
    UnifiedNum,
};
use tokio_postgres::{types::ToSql, Row};

use super::{DbPool, PoolError};

pub async fn get_analytics(
    pool: &DbPool,
    query: AnalyticsQuery,
    allowed_keys: HashSet<AllowedKey>,
    auth_as: Option<AuthenticateAs>,
    limit: u32,
) -> Result<Vec<FetchedAnalytics>, PoolError> {
    let client = pool.get().await?;

    let (where_clauses, params) = analytics_query_params(&query, auth_as.as_ref(), &allowed_keys);

    // "make_timestamp()";
    // make_timestamptz(COALESCE(2013, 7, 15, 8, 15, 23.5, 'UTC')

    let time_group = match &query.time.timeframe {
        Timeframe::Year => "date_trunc('month', analytics.time) as timeframe_time",
        Timeframe::Month => "date_trunc('day', analytics.time) as timeframe_time",
        Timeframe::Week | Timeframe::Day => "date_trunc('hour', analytics.time) as timeframe_time",
    };
    // let time_group = {
    //     let extract_from =
    //         // |part: &str| -> String { format!("EXTRACT({} FROM TIMESTAMPTZ time)", part) };
    //         |part: &str| -> String { format!("EXTRACT({} FROM time)", part) };

    //     match &query.time.timeframe {
    //         Timeframe::Year => vec![extract_from("YEAR"), extract_from("MONTH")],
    //         Timeframe::Month => vec![
    //             extract_from("YEAR"),
    //             extract_from("MONTH"),
    //             extract_from("DAY"),
    //         ],
    //         Timeframe::Week | Timeframe::Day => vec![
    //             extract_from("YEAR"),
    //             extract_from("MONTH"),
    //             extract_from("DAY"),
    //             extract_from("HOUR"),
    //         ],
    //     }
    // };

    let mut select_clause = vec![time_group.to_string()];
    match &query.metric {
        Metric::Paid => select_clause.push("SUM(payout_amount)::bigint as value".to_string()),
        Metric::Count => select_clause.push("SUM(payout_count)::integer as value".to_string()),
    }
    // always group first by the **date_trunc** time
    let mut group_clause = vec!["timeframe_time".to_string()];

    // if segment by was passed, use it in group second & select by it
    if let Some(segment_by) = &query.segment_by {
        select_clause.push(segment_by.to_string());
        group_clause.push(segment_by.to_string());
    }

    let sql_query = format!(
        "SELECT {} FROM analytics WHERE {} GROUP BY {} ORDER BY timeframe_time ASC LIMIT {}",
        select_clause.join(", "),
        where_clauses.join(" AND "),
        group_clause.join(", "),
        limit,
    );

    // execute query
    let stmt = client.prepare(dbg!(&sql_query)).await?;
    let rows: Vec<Row> = client.query_raw(&stmt, params).await?.try_collect().await?;

    let analytics: Vec<FetchedAnalytics> = rows
        .iter()
        .map(|row| {
            let query = query.clone();

            // Since segment_by is a dynamic value/type it can't be passed to from<&Row> so we're building the object here
            let segment_value = match query.segment_by.as_ref() {
                Some(segment_by) => row.try_get(segment_by.to_string().as_str()).ok(),
                None => None,
            };
            let time = row.get::<_, DateTime<Utc>>("timeframe_time");
            let value = match &query.metric {
                Metric::Paid => row.get("value"),
                Metric::Count => {
                    let count: i32 = row.get("value");
                    UnifiedNum::from_u64(u64::from(count.unsigned_abs()))
                }
            };
            FetchedAnalytics {
                time,
                value,
                segment: segment_value,
            }
        })
        .collect();

    Ok(analytics)
}

fn analytics_query_params(
    query: &AnalyticsQuery,
    auth_as: Option<&AuthenticateAs>,
    allowed_keys: &HashSet<AllowedKey>,
) -> (Vec<String>, Vec<Box<(dyn ToSql + Sync + Send)>>) {
    let mut where_clauses = vec!["\"time\" >= $1".to_string()];
    let mut params: Vec<Box<(dyn ToSql + Sync + Send)>> = vec![Box::new(query.time.start.clone())];

    // for all allowed keys for this query
    // insert parameter into the SQL if there is a value set for it
    for allowed_key in allowed_keys.iter() {
        let value = query.get_key(*allowed_key);
        if let Some(value) = value {
            where_clauses.push(format!("{} = ${}", allowed_key, params.len() + 1));
            params.push(value);
        }
    }

    // IMPORTANT FOR SECURITY: this must be LAST so that query.publisher/query.advertiser cannot override it
    match auth_as {
        Some(AuthenticateAs::Publisher(uid)) => {
            where_clauses.push(format!("publisher = ${}", params.len() + 1));
            params.push(Box::new(*uid));
        }
        Some(AuthenticateAs::Advertiser(uid)) => {
            where_clauses.push(format!("advertiser = ${}", params.len() + 1));
            params.push(Box::new(*uid));
        }
        _ => {}
    }

    if let Some(end_date) = &query.time.end {
        where_clauses.push(format!("\"time\" <= ${}", params.len() + 1));
        params.push(Box::new(end_date.clone()));
    }
    where_clauses.push(format!("event_type = ${}", params.len() + 1));
    params.push(Box::new(query.event_type.clone()));

    where_clauses.push(format!("{} IS NOT NULL", query.metric.column_name()));

    (where_clauses, params)
}

/// This will update a record when it's present by incrementing its payout_amount and payout_count fields
pub async fn update_analytics(
    pool: &DbPool,
    update_analytics: UpdateAnalytics,
) -> Result<Analytics, PoolError> {
    let client = pool.get().await?;

    let query = "INSERT INTO analytics(campaign_id, time, ad_unit, ad_slot, ad_slot_type, advertiser, publisher, hostname, country, os_name, event_type, payout_amount, payout_count)
    VALUES ($1, date_trunc('hour', cast($2 as timestamp with time zone)), $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
    ON CONFLICT ON CONSTRAINT analytics_pkey DO UPDATE
    SET payout_amount = analytics.payout_amount + $12, payout_count = analytics.payout_count + 1
    RETURNING campaign_id, time, ad_unit, ad_slot, ad_slot_type, advertiser, publisher, hostname, country, os_name, event_type, payout_amount, payout_count";

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
        analytics::{OperatingSystem, query::{Time, ALLOWED_KEYS}},
        sentry::DateHour,
        util::tests::prep_db::{ADDRESSES, DUMMY_AD_UNITS, DUMMY_CAMPAIGN, DUMMY_IPFS},
        UnifiedNum, test_util::{PUBLISHER, CREATOR}, IPFS, AdUnit,
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

    #[tokio::test]
    async fn test_get_analytics() {
        let database = DATABASE_POOL.get().await.expect("Should get a DB pool");

        let ad_unit = DUMMY_AD_UNITS[0].clone();
        let ad_slot_ipfs = DUMMY_IPFS[0];
        
        setup_test_migrations(database.pool.clone())
        .await
        .expect("Migrations should succeed");

        generate_analytics(&database.pool, &ad_unit, ad_slot_ipfs).await;

        let start_date = DateHour::from_ymdh(2021, 12, 1, 14);
        let end_date = DateHour::from_ymdh(2022, 1, 1, 0);

        let query = AnalyticsQuery {
            limit: 1000,
            event_type: "IMPRESSION".into(),
            metric: Metric::Count,
            segment_by: Some(AllowedKey::Country),
            time: Time {
                timeframe: Timeframe::Day,
                start: start_date,
                end: Some(end_date),
            },
            campaign_id: Some(DUMMY_CAMPAIGN.id),
            ad_unit: Some(ad_unit.ipfs),
            ad_slot: Some(ad_slot_ipfs),
            ad_slot_type: Some(ad_unit.ad_type.clone()),
            advertiser: Some(*CREATOR),
            publisher: Some(*PUBLISHER),
            hostname: Some("localhost".into()),
            country: Some("Bulgaria".into()),
            os_name: Some(OperatingSystem::Linux),
        };


        let results = get_analytics(&database.pool, query.clone(), ALLOWED_KEYS.clone(), None, query.limit).await.expect("Should fetch");
        dbg!(results);
    }

    async fn generate_analytics(database: &DbPool, ad_unit: &AdUnit, ad_slot: IPFS) {
        // let start = DateHour::from_ymdh(2021, 12, 1, 0);
        for day in 1..=31_u32 {
            for hour in 0..=23_u32 {
                let analytics = UpdateAnalytics {
                    time: DateHour::from_ymdh(2021, 12, day, hour),
                    campaign_id: DUMMY_CAMPAIGN.id,
                    ad_unit: Some(ad_unit.ipfs),
                    ad_slot: Some(ad_slot),
                    ad_slot_type: Some(ad_unit.ad_type.clone()),
                    advertiser: *CREATOR,
                    publisher: *PUBLISHER,
                    hostname: Some("localhost".to_string()),
                    country: Some("Bulgaria".to_string()),
                    os_name: OperatingSystem::Linux,
                    event_type: "IMPRESSION".to_string(),
                    amount_to_add: UnifiedNum::from_u64(day as u64 * hour as u64 * 1_000_000),
                    count_to_add: hour as i32,
                };
        
                update_analytics(&database.clone(), analytics.clone())
                    .await
                    .expect("Should insert");
            }
            
        }

        
    }
}
