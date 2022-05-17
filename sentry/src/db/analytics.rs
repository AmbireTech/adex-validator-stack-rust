use std::collections::HashSet;

use futures::TryStreamExt;
use primitives::{
    analytics::{query::AllowedKey, AnalyticsQuery, AuthenticateAs, Metric, Timeframe},
    sentry::{Analytics, FetchedAnalytics, UpdateAnalytics},
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

    let time_group = match &query.time.timeframe {
        Timeframe::Year => "date_trunc('month', analytics.time) as timeframe_time",
        Timeframe::Month => "date_trunc('day', analytics.time) as timeframe_time",
        Timeframe::Week | Timeframe::Day => "date_trunc('hour', analytics.time) as timeframe_time",
    };

    let mut select_clause = vec![time_group.to_string()];
    match &query.metric {
        Metric::Paid => select_clause.push("SUM(payout_amount)::bigint as value".to_string()),
        Metric::Count => select_clause.push("SUM(payout_count)::integer as value".to_string()),
    }
    // always group first by the **date_trunc** time
    let mut group_clause = vec!["timeframe_time".to_string()];

    // if segment by was passed, use it in group second & select by it
    if let Some(segment_by) = &query.segment_by {
        select_clause.push(format!("{} as segment_by", segment_by));
        group_clause.push("segment_by".into());
    }

    let sql_query = format!(
        "SELECT {} FROM analytics WHERE {} GROUP BY {} ORDER BY timeframe_time ASC LIMIT {}",
        select_clause.join(", "),
        where_clauses.join(" AND "),
        group_clause.join(", "),
        limit,
    );

    // Prepare SQL statement
    let stmt = client.prepare(&sql_query).await?;
    // Execute query
    let rows: Vec<Row> = client.query_raw(&stmt, params).await?.try_collect().await?;

    // FetchedAnalytics requires context using the `AnalyticsQuery`
    // this is why we use `impl From<(&AnalyticsQuery, &Row)>`
    let analytics = rows
        .iter()
        .map(|row| FetchedAnalytics::from((&query, row)))
        .collect();

    Ok(analytics)
}

fn analytics_query_params(
    query: &AnalyticsQuery,
    auth_as: Option<&AuthenticateAs>,
    allowed_keys: &HashSet<AllowedKey>,
) -> (Vec<String>, Vec<Box<(dyn ToSql + Sync + Send)>>) {
    let mut where_clauses = vec!["\"time\" >= $1".to_string()];
    let mut params: Vec<Box<(dyn ToSql + Sync + Send)>> = vec![Box::new(query.time.start)];

    // for all allowed keys of this query
    // insert parameter into the SQL if there is a value set for it
    for allowed_key in allowed_keys.iter() {
        // IMPORTANT FOR SECURITY: AuthenticateAs must OVERRIDE the value passed to either query.publisher or query.advertiser respectively
        let value = match (allowed_key, auth_as) {
            (AllowedKey::Advertiser, Some(AuthenticateAs::Advertiser(advertiser))) => {
                Some(Box::new(*advertiser) as _)
            }
            (AllowedKey::Publisher, Some(AuthenticateAs::Publisher(publisher))) => {
                Some(Box::new(*publisher) as _)
            }
            _ => query.get_key(*allowed_key),
        };

        if let Some(value) = value {
            where_clauses.push(format!("{} = ${}", allowed_key, params.len() + 1));
            params.push(value);
        }
    }

    if let Some(end_date) = &query.time.end {
        where_clauses.push(format!("\"time\" <= ${}", params.len() + 1));
        params.push(Box::new(*end_date));
    }
    where_clauses.push(format!("event_type = ${}", params.len() + 1));
    params.push(Box::new(query.event_type));

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
                &update_analytics.ad_unit,
                &update_analytics.ad_slot,
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
    use std::collections::HashMap;

    use super::*;
    use primitives::{
        analytics::{
            query::{Time, ALLOWED_KEYS},
            OperatingSystem,
        },
        sentry::{DateHour, CLICK, IMPRESSION},
        test_util::{CREATOR, DUMMY_AD_UNITS, DUMMY_CAMPAIGN, DUMMY_IPFS, PUBLISHER, PUBLISHER_2},
        unified_num::FromWhole,
        AdUnit, UnifiedNum, ValidatorId, IPFS,
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
                ad_unit: ad_unit.ipfs,
                ad_slot: ad_slot_ipfs,
                ad_slot_type: Some(ad_unit.ad_type.clone()),
                advertiser: *CREATOR,
                publisher: *PUBLISHER,
                hostname: Some("localhost".to_string()),
                country: Some("Bulgaria".to_string()),
                os_name: OperatingSystem::Linux,
                event_type: IMPRESSION,
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
                ad_unit: DUMMY_IPFS[1],
                ad_slot: DUMMY_IPFS[0],
                ad_slot_type: None,
                advertiser: *CREATOR,
                publisher: *PUBLISHER,
                hostname: None,
                country: None,
                os_name: OperatingSystem::Linux,
                event_type: IMPRESSION,
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
    async fn test_get_analytics_in_december() {
        let database = DATABASE_POOL.get().await.expect("Should get a DB pool");

        let ad_unit = DUMMY_AD_UNITS[0].clone();
        let ad_unit_2 = DUMMY_AD_UNITS[1].clone();
        let ad_slot_ipfs = DUMMY_IPFS[0];

        setup_test_migrations(database.pool.clone())
            .await
            .expect("Migrations should succeed");

        let start_date = DateHour::from_ymdh(2021, 12, 1, 0);
        let end_date = DateHour::from_ymdh(2022, 12, 31, 23);

        let december_days = 1..=31_u32;
        let hours = 0..=23_u32;

        generate_analytics_for_december_2021(&database.pool, &ad_unit, ad_slot_ipfs).await;
        //
        //
        // TODO: throw a few more analytics in specific DateHours w/ unique Query and check if it filters the analytics correctly
        let other_analytics: HashMap<&str, Analytics> = {
            // 5.12.2021 16:00
            let mut click_germany = make_click_analytics(&ad_unit, ad_slot_ipfs, 5, 16);
            click_germany.country = Some("Germany".into());
            let click_germany = update_analytics(&database.pool, click_germany)
                .await
                .expect("Should update");

            // 6.12.2021 9:00
            let impression_ad_unit_2 = make_impression_analytics(&ad_unit_2, ad_slot_ipfs, 6, 9);
            let impression_ad_unit_2 = update_analytics(&database.pool, impression_ad_unit_2)
                .await
                .expect("Should update");

            // 7.12.2021 21:00
            let mut impression_publisher_2 =
                make_impression_analytics(&ad_unit, ad_slot_ipfs, 7, 21);
            impression_publisher_2.publisher = *PUBLISHER_2;
            let impression_publisher_2 = update_analytics(&database.pool, impression_publisher_2)
                .await
                .expect("Should update");

            vec![
                ("click_germany", click_germany),
                ("impression_ad_unit_2", impression_ad_unit_2),
                ("impression_publisher_2", impression_publisher_2),
            ]
            .into_iter()
            .collect()
        };
        //
        //

        let amount_per_day: UnifiedNum = hours
            .clone()
            .map(|hour| UnifiedNum::from_whole(hour as u64))
            .sum::<Option<_>>()
            .expect("Should not overflow");
        let amount_for_month: UnifiedNum = december_days
            .clone()
            .map(|day_n| amount_per_day * UnifiedNum::from_whole(day_n as u64))
            .sum::<Option<UnifiedNum>>()
            .expect("Should not overflow");

        let count_sum = |fetched_analytics: &[FetchedAnalytics]| -> u32 {
            fetched_analytics
                .iter()
                .map(|analytics| analytics.value.get_count().expect("Should be Count"))
                .sum()
        };

        let count_per_day: u32 = hours.sum();
        // 276 events per day
        assert_eq!(276, count_per_day);

        let impression_query = AnalyticsQuery {
            limit: 1000,
            event_type: IMPRESSION,
            metric: Metric::Count,
            segment_by: Some(AllowedKey::Country),
            time: Time {
                timeframe: Timeframe::Month,
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

        // Impression query - should count all inserted Analytics
        {
            let count_impressions = get_analytics(
                &database.pool,
                impression_query.clone(),
                ALLOWED_KEYS.clone(),
                None,
                impression_query.limit,
            )
            .await
            .expect("Should fetch");

            // check at least 1 day of the month to validate the expected count
            let first_day_count = count_impressions
                .get(0)
                .expect("Should have index 0")
                .value
                .get_count()
                .expect("Should be Count");

            assert_eq!(count_per_day, first_day_count);

            // All Counts for 31 days should match
            assert_eq!(31 * count_per_day, count_sum(&count_impressions));

            // Check 2 days in the month
            // 2.12.2021 & 3.12.2021
            let two_days_fetched_count = count_impressions
                // indices should match with 2nd and 3rd December
                .get(2..=3)
                .expect("Should have indices of the range")
                .iter()
                .map(|fetched| {
                    fetched
                        .value
                        .get_count()
                        .expect("Should be a Metric::Count")
                })
                .sum::<u32>();

            assert_eq!(2 * count_per_day, two_days_fetched_count);

            let mut paid_impressions_query = impression_query.clone();
            paid_impressions_query.metric = Metric::Paid;

            let paid_impressions = get_analytics(
                &database.pool,
                paid_impressions_query,
                ALLOWED_KEYS.clone(),
                None,
                impression_query.limit,
            )
            .await
            .expect("Should fetch Metric::Paid of Impressions");

            assert_eq!(
                paid_impressions.iter()
                .map(|analytics| analytics.value.get_paid().expect("Should be Metric::Paid"))
                    .sum::<Option<UnifiedNum>>()
                    .expect("Should not overflow"),
                amount_for_month,
                "The sum of all the Fetched Analytics paid out values should match with the expected for this month"
            );

            // Check 3 days of the month
            // 14.12.2021 & 15.12.2021 & 16.12.2021
            let three_days_fetched_paid = paid_impressions
                // indices should match with 14th, 15th and 16th of December
                .get(13..=15)
                .expect("Should have indices of the range")
                .iter()
                .map(|fetched| fetched.value.get_paid().expect("Should be a Metric::Paid"))
                .sum::<Option<UnifiedNum>>()
                .expect("Should not overflow");

            assert_eq!(
                UnifiedNum::from_whole(14) * amount_per_day
                    + UnifiedNum::from_whole(15) * amount_per_day
                    + UnifiedNum::from_whole(16) * amount_per_day,
                three_days_fetched_paid
            );
        }

        let click_query = AnalyticsQuery {
            limit: 1000,
            event_type: CLICK,
            metric: Metric::Count,
            segment_by: Some(AllowedKey::Country),
            time: Time {
                timeframe: Timeframe::Month,
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
            country: Some("Estonia".into()),
            os_name: Some(OperatingSystem::Linux),
        };

        // Click query - should count all inserted Analytics
        {
            let count_clicks = get_analytics(
                &database.pool,
                click_query.clone(),
                ALLOWED_KEYS.clone(),
                None,
                click_query.limit,
            )
            .await
            .expect("Should fetch");

            // check at least 1 day of the month to validate the expected count
            let second_day_count = count_clicks
                .get(1)
                .expect("Should have index 1 corresponding to 2.12.2021")
                .value
                .get_count()
                .expect("Should be Count");

            assert_eq!(count_per_day, second_day_count);

            // All Counts for 31 days should match
            assert_eq!(31 * count_per_day, count_sum(&count_clicks));

            // Check 2 days in the month
            // 5.12.2021 & 6.12.2021
            let two_days_fetched_count = count_clicks
                // indices should match with 5nd and 6rd December
                .get(5..=6)
                .expect("Should have indices of the range")
                .iter()
                .map(|fetched| {
                    fetched
                        .value
                        .get_count()
                        .expect("Should be a Metric::Count")
                })
                .sum::<u32>();

            assert_eq!(2 * count_per_day, two_days_fetched_count);

            let mut paid_clicks_query = impression_query.clone();
            paid_clicks_query.metric = Metric::Paid;

            let paid_impressions = get_analytics(
                &database.pool,
                paid_clicks_query,
                ALLOWED_KEYS.clone(),
                None,
                impression_query.limit,
            )
            .await
            .expect("Should fetch Metric::Paid of Impressions");

            assert_eq!(
                paid_impressions.iter()
                .map(|analytics| analytics.value.get_paid().expect("Should be Metric::Paid"))
                    .sum::<Option<UnifiedNum>>()
                    .expect("Should not overflow"),
                amount_for_month,
                "The sum of all the Fetched Analytics paid out values should match with the expected for this month"
            );

            // Check 6 days in the month
            // 10, 11, 12, 13, 14.12.2021 & 15.12.2021
            let six_days_fetched_paid = paid_impressions
                // indices don't match with dates, because they start at index 0
                .get(9..=14)
                .expect("Should have indices of the range")
                .iter()
                .map(|fetched| fetched.value.get_paid().expect("Should be a Metric::Paid"))
                .sum::<Option<UnifiedNum>>()
                .expect("Should not overflow");

            assert_eq!(
                UnifiedNum::from_whole(10) * amount_per_day
                    + UnifiedNum::from_whole(11) * amount_per_day
                    + UnifiedNum::from_whole(12) * amount_per_day
                    + UnifiedNum::from_whole(13) * amount_per_day
                    + UnifiedNum::from_whole(14) * amount_per_day
                    + UnifiedNum::from_whole(15) * amount_per_day,
                six_days_fetched_paid
            );
        }

        // Filter by country: Germany
        // Type: Click
        // DateHour 5.12.2021 16:00
        // Only a single analytics should show for this day
        {
            let mut click_germany_query = click_query.clone();
            click_germany_query.time = Time {
                timeframe: Timeframe::Day,
                start: DateHour::from_ymdh(2021, 12, 5, 0),
                end: Some(DateHour::from_ymdh(2021, 12, 6, 0)),
            };
            click_germany_query.country = Some("Germany".into());

            let count_clicks = get_analytics(
                &database.pool,
                click_germany_query.clone(),
                ALLOWED_KEYS.clone(),
                None,
                click_germany_query.limit,
            )
            .await
            .expect("Should fetch");

            assert_eq!(1, count_clicks.len(), "Only single analytics is expected");
            let fetched = count_clicks.get(0).expect("Should have index 0");
            assert_eq!(
                16,
                fetched.value.get_count().expect("Should be Metric::Count"),
                "The fetched count should be the same as the hour of the Analytics"
            );
            assert_eq!(
                other_analytics["click_germany"].time.to_datetime(),
                fetched.time,
                "The fetched analytics time (with hour) should be the same as the inserted Analytics"
            );
        }

        // Filter only by AdUnit 2
        // Type: Impression
        // DateHour 6.12.2021 9:00
        // Only a single analytics should show
        {
            let ad_unit_2_query = AnalyticsQuery {
                limit: 1000,
                event_type: IMPRESSION,
                metric: Metric::Count,
                segment_by: Some(AllowedKey::Country),
                time: Time {
                    timeframe: Timeframe::Day,
                    start: start_date,
                    end: Some(end_date),
                },
                campaign_id: None,
                ad_unit: Some(ad_unit_2.ipfs),
                ad_slot: None,
                ad_slot_type: None,
                advertiser: None,
                publisher: None,
                hostname: None,
                country: None,
                os_name: None,
            };

            let count_impressions = get_analytics(
                &database.pool,
                ad_unit_2_query.clone(),
                ALLOWED_KEYS.clone(),
                None,
                ad_unit_2_query.limit,
            )
            .await
            .expect("Should fetch");

            assert_eq!(
                1,
                count_impressions.len(),
                "Only single analytics is expected"
            );
            let fetched = count_impressions.get(0).expect("Should have index 0");
            assert_eq!(
                9,
                fetched.value.get_count().expect("Should be Metric::Count"),
                "The fetched count should be the same as the hour of the Analytics"
            );
            assert_eq!(
                other_analytics["impression_ad_unit_2"].time.to_datetime(),
                fetched.time,
                "The fetched analytics date should be the same as the inserted Analytics, but with no hour because Timeframe::Day"
            );
        }

        // Filter by PUBLISHER_2
        //
        // Type: Impression
        // DateHour 7.12.2021 21:00
        // Only a single analytics should show
        {
            let filter_by_publisher_2_query = AnalyticsQuery {
                limit: 1000,
                event_type: IMPRESSION,
                metric: Metric::Count,
                segment_by: Some(AllowedKey::Publisher),
                time: Time {
                    timeframe: Timeframe::Day,
                    start: DateHour::from_ymdh(2021, 12, 7, 0),
                    end: Some(DateHour::from_ymdh(2021, 12, 7, 23)),
                },
                campaign_id: None,
                ad_unit: None,
                ad_slot: None,
                ad_slot_type: None,
                advertiser: None,
                publisher: Some(*PUBLISHER_2),
                hostname: None,
                country: None,
                os_name: None,
            };

            let count_impressions = get_analytics(
                &database.pool,
                filter_by_publisher_2_query.clone(),
                ALLOWED_KEYS.clone(),
                None,
                filter_by_publisher_2_query.limit,
            )
            .await
            .expect("Should fetch");

            assert_eq!(
                1,
                count_impressions.len(),
                "Should fetch the single analytics with different publisher"
            );
            assert_eq!(
                21,
                count_impressions
                    .get(0)
                    .expect("Should get index 0")
                    .value
                    .get_count()
                    .expect("Should be Metric::Count"),
                "Count value of Analytics should be the same as it's hour value"
            );
        }

        // AuthenticateAs by PUBLISHER, should override the query value of `publisher`
        //
        // Type: Impression
        // DateHour 7.12.2021 21:00
        // Only a single analytics should show
        {
            let authenticate_as_publisher_2_query = AnalyticsQuery {
                limit: 1000,
                event_type: IMPRESSION,
                metric: Metric::Count,
                segment_by: Some(AllowedKey::Publisher),
                time: Time {
                    timeframe: Timeframe::Day,
                    start: DateHour::from_ymdh(2021, 12, 7, 0),
                    end: Some(DateHour::from_ymdh(2021, 12, 7, 23)),
                },
                campaign_id: None,
                ad_unit: None,
                ad_slot: None,
                ad_slot_type: None,
                advertiser: None,
                // AuthenticateAs should override this value!
                publisher: Some(*PUBLISHER),
                hostname: None,
                country: None,
                os_name: None,
            };

            let count_impressions = get_analytics(
                &database.pool,
                authenticate_as_publisher_2_query.clone(),
                ALLOWED_KEYS.clone(),
                Some(AuthenticateAs::Publisher(ValidatorId::from(*PUBLISHER_2))),
                authenticate_as_publisher_2_query.limit,
            )
            .await
            .expect("Should fetch");

            assert_eq!(
                1,
                count_impressions.len(),
                "Should fetch the single analytics with different publisher"
            );
            assert_eq!(
                21,
                count_impressions
                    .get(0)
                    .expect("Should get index 0")
                    .value
                    .get_count()
                    .expect("Should be Metric::Count"),
                "Count value of Analytics should be the same as it's hour value"
            );
        }

        // Segment by AdUnit
        // Type: Impression
        // DateHour for AdUnit 2: 6.12.2021 9:00
        // we should get 2 distinct AdUnits:
        // - Analytics generated for the full month
        // - One for a single DateHour that adds single Analytics
        {
            let segment_ad_units_query = AnalyticsQuery {
                limit: 1000,
                event_type: IMPRESSION,
                metric: Metric::Count,
                segment_by: Some(AllowedKey::AdUnit),
                time: Time {
                    timeframe: Timeframe::Day,
                    start: DateHour::from_ymdh(2021, 12, 6, 0),
                    end: Some(DateHour::from_ymdh(2021, 12, 6, 23)),
                },
                campaign_id: None,
                ad_unit: None,
                ad_slot: None,
                ad_slot_type: None,
                advertiser: None,
                publisher: None,
                hostname: None,
                country: None,
                os_name: None,
            };

            let count_impressions = get_analytics(
                &database.pool,
                segment_ad_units_query.clone(),
                ALLOWED_KEYS.clone(),
                None,
                segment_ad_units_query.limit,
            )
            .await
            .expect("Should fetch");

            assert_eq!(
                25,
                count_impressions.len(),
                "We expect a total of 24 + 1 (with different AdUnit) analytics"
            );
            let find_ad_unit_2 = count_impressions
                .iter()
                .find(|fetched| fetched.segment == Some(ad_unit_2.ipfs.to_string()))
                .expect("There should be a single FetchedAnalytics with different AdUnit");

            assert_eq!(
                9,
                find_ad_unit_2
                    .value
                    .get_count()
                    .expect("Should be Metric::Count"),
                "It's count should be == to the hour"
            )
        }
    }

    /// Makes an [`IMPRESSION`] [`UpdateAnalytics`] for testing.
    /// Country "Bulgaria"
    /// Everything else is the same as [`make_click_analytics`].
    fn make_impression_analytics(
        ad_unit: &AdUnit,
        ad_slot: IPFS,
        day: u32,
        hour: u32,
    ) -> UpdateAnalytics {
        UpdateAnalytics {
            time: DateHour::from_ymdh(2021, 12, day, hour),
            campaign_id: DUMMY_CAMPAIGN.id,
            ad_unit: ad_unit.ipfs,
            ad_slot: ad_slot,
            ad_slot_type: Some(ad_unit.ad_type.clone()),
            advertiser: *CREATOR,
            publisher: *PUBLISHER,
            hostname: Some("localhost".to_string()),
            country: Some("Bulgaria".to_string()),
            os_name: OperatingSystem::Linux,
            event_type: IMPRESSION,
            amount_to_add: UnifiedNum::from_u64(day as u64 * hour as u64 * 100_000_000),
            count_to_add: hour as i32,
        }
    }

    /// Makes an [`CLICK`] [`UpdateAnalytics`] for testing.
    /// Country "Estonia"
    /// Everything else is the same as [`make_click_analytics`].
    fn make_click_analytics(
        ad_unit: &AdUnit,
        ad_slot: IPFS,
        day: u32,
        hour: u32,
    ) -> UpdateAnalytics {
        UpdateAnalytics {
            time: DateHour::from_ymdh(2021, 12, day, hour),
            campaign_id: DUMMY_CAMPAIGN.id,
            ad_unit: ad_unit.ipfs,
            ad_slot: ad_slot,
            ad_slot_type: Some(ad_unit.ad_type.clone()),
            advertiser: *CREATOR,
            publisher: *PUBLISHER,
            hostname: Some("localhost".to_string()),
            country: Some("Estonia".to_string()),
            os_name: OperatingSystem::Linux,
            event_type: CLICK,
            amount_to_add: UnifiedNum::from_u64(day as u64 * hour as u64 * 100_000_000),
            count_to_add: hour as i32,
        }
    }

    /// Creates Impression & Click analytics for each hour of December
    async fn generate_analytics_for_december_2021(
        database: &DbPool,
        ad_unit: &AdUnit,
        ad_slot: IPFS,
    ) {
        for day in 1..=31_u32 {
            for hour in 0..=23_u32 {
                let impression_analytics = make_impression_analytics(ad_unit, ad_slot, day, hour);

                update_analytics(&database.clone(), impression_analytics)
                    .await
                    .expect("Should insert");

                let click_analytics = make_click_analytics(ad_unit, ad_slot, day, hour);

                update_analytics(&database.clone(), click_analytics)
                    .await
                    .expect("Should insert");
            }
        }
    }
}
