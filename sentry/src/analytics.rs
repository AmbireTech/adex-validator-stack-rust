use crate::{
    db::{analytics::insert_analytics, DbPool, PoolError},
    Session,
};
use chrono::{Timelike, Utc};
use primitives::{
    analytics::OperatingSystem,
    sentry::{DateHour, Event, UpdateAnalytics},
    Address, Campaign, UnifiedNum,
};
use std::collections::HashSet;

/// Validator fees will not be included in analytics
pub async fn record(
    pool: &DbPool,
    campaign: &Campaign,
    session: &Session,
    events_with_payouts: Vec<(Event, Address, UnifiedNum)>,
) -> Result<(), PoolError> {
    let os_name = session
        .os
        .as_ref()
        .map(|os| OperatingSystem::map_os(os))
        .unwrap_or_default();
    let time = {
        let full_utc = Utc::now();

        // leave only the Hour portion and erase the minutes & seconds
        DateHour {
            date: full_utc.date().and_hms(0, 0, 0), // TODO: Fix
            hour: full_utc.hour(),
        }
    };

    let mut analytics_set: HashSet<UpdateAnalytics> = HashSet::new();
    for (event, _payout_addr, payout_amount) in events_with_payouts {
        let event_type = event.to_string();
        let (publisher, ad_unit, referrer, ad_slot, ad_slot_type) = {
            let (publisher, event_ad_unit, referrer, ad_slot) = match event {
                Event::Impression {
                    publisher,
                    ad_unit,
                    referrer,
                    ad_slot,
                } => (publisher, ad_unit, referrer, ad_slot),
                Event::Click {
                    publisher,
                    ad_unit,
                    referrer,
                    ad_slot,
                } => (publisher, ad_unit, referrer, ad_slot),
            };
            let ad_unit = event_ad_unit.and_then(|ipfs| {
                campaign
                    .ad_units
                    .iter()
                    .find(|ad_unit| ad_unit.ipfs == ipfs)
            });
            let ad_slot_type = ad_unit.as_ref().map(|unit| unit.ad_type.clone());
            (publisher, event_ad_unit, referrer, ad_slot, ad_slot_type)
        };

        let hostname = match (&referrer, session.referrer_header.as_ref()) {
            (Some(referrer), None) | (None, Some(referrer)) | (Some(referrer), Some(_)) => referrer
                .split('/')
                .collect::<Vec<&str>>()
                .get(2)
                .map(|hostname| hostname.to_string()),

            (None, None) => None,
        };

        // DB: Insert or Update all events
        let mut analytics = UpdateAnalytics {
            campaign_id: campaign.id,
            time,
            ad_unit,
            ad_slot,
            ad_slot_type,
            advertiser: campaign.creator,
            publisher,
            hostname,
            country: session.country.to_owned(),
            os_name: os_name.clone(),
            event_type,
            amount_to_add: payout_amount,
            count_to_add: 1,
        };
        // TODO: tidy up this operation
        match analytics_set.get(&analytics) {
            Some(a) => {
                analytics.amount_to_add += &a.amount_to_add;
                analytics.count_to_add = a.count_to_add + 1;
                let _ = &analytics_set.replace(a.to_owned());
            }
            None => {
                let _ = &analytics_set.insert(analytics);
            }
        }
    }
    for a in analytics_set.iter() {
        insert_analytics(pool, a).await?;
    }
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    use primitives::{
        util::tests::prep_db::{ADDRESSES, DUMMY_CAMPAIGN},
        UnifiedNum,
    };

    use crate::db::{
        analytics::find_analytics,
        tests_postgres::{setup_test_migrations, DATABASE_POOL},
    };

    // NOTE: The test could fail if it's ran at --:59:59
    #[tokio::test]
    async fn test_analytics_recording() {
        let database = DATABASE_POOL.get().await.expect("Should get a DB pool");

        setup_test_migrations(database.pool.clone())
            .await
            .expect("Migrations should succeed");

        let campaign = DUMMY_CAMPAIGN.clone();
        let session = Session {
            ip: None,
            country: None,
            referrer_header: None,
            os: None,
        };

        let click_event = Event::Click {
            publisher: ADDRESSES["leader"],
            ad_unit: None,
            ad_slot: None,
            referrer: None,
        };

        let impression_event = Event::Impression {
            publisher: ADDRESSES["leader"],
            ad_unit: None,
            ad_slot: None,
            referrer: None,
        };

        let input_events = vec![
            (
                click_event,
                ADDRESSES["creator"],
                UnifiedNum::from_u64(1_000_000),
            ),
            (
                impression_event,
                ADDRESSES["creator"],
                UnifiedNum::from_u64(1_000_000),
            ),
        ];

        record(&database.clone(), &campaign, &session, input_events.clone())
            .await
            .expect("should record");

        let analytics = find_analytics(&database.pool)
            .await
            .expect("should find analytics");
        let click_analytics = analytics
            .iter()
            .find(|a| a.event_type == "CLICK")
            .expect("There should be a click event");
        let impression_analytics = analytics
            .iter()
            .find(|a| a.event_type == "IMPRESSION")
            .expect("There should be an impression event");
        assert_eq!(
            click_analytics.payout_amount,
            UnifiedNum::from_u64(1_000_000)
        );
        assert_eq!(click_analytics.payout_count, 1);

        assert_eq!(
            impression_analytics.payout_amount,
            UnifiedNum::from_u64(1_000_000)
        );
        assert_eq!(impression_analytics.payout_count, 1);

        record(&database.clone(), &campaign, &session, input_events)
            .await
            .expect("should record");

        let analytics = find_analytics(&database.pool)
            .await
            .expect("should find analytics");
        let click_analytics = analytics
            .iter()
            .find(|a| a.event_type == "CLICK")
            .expect("There should be a click event");
        let impression_analytics = analytics
            .iter()
            .find(|a| a.event_type == "IMPRESSION")
            .expect("There should be an impression event");
        assert_eq!(
            click_analytics.payout_amount,
            UnifiedNum::from_u64(2_000_000)
        );
        assert_eq!(click_analytics.payout_count, 2);

        assert_eq!(
            impression_analytics.payout_amount,
            UnifiedNum::from_u64(2_000_000)
        );
        assert_eq!(impression_analytics.payout_count, 2);
    }
}
