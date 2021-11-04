use crate::{
    db::{analytics::insert_analytics, DbPool, PoolError},
    Session,
};
use chrono::Utc;
use primitives::{
    analytics::OperatingSystem,
    sentry::{Event, EventAnalytics},
    Address, Campaign, UnifiedNum,
};

/// Validator fees will not be included in analytics
pub async fn record(
    pool: &DbPool,
    campaign: &Campaign,
    session: &Session,
    events_with_payouts: Vec<(Event, Address, UnifiedNum)>,
) -> Result<(), PoolError> {
    let os_name = OperatingSystem::map_os(session.os.as_ref().unwrap_or(&"".to_string()));
    let time = Utc::now();

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
        let event_for_db = EventAnalytics {
            campaign_id: campaign.id,
            time,
            ad_unit,
            ad_slot,
            ad_slot_type,
            advertiser: campaign.creator,
            publisher,
            hostname,
            country: session.country.to_owned(),
            os_name: os_name.to_owned(),
            event_type,
            payout_amount,
        };

        insert_analytics(pool, &event_for_db).await?;
    }

    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    use primitives::{
        analytics::OperatingSystem,
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
            .expect("should recorc");

        let query_click_event = EventAnalytics {
            time: Utc::now(),
            campaign_id: DUMMY_CAMPAIGN.id,
            ad_unit: None,
            ad_slot: None,
            ad_slot_type: None,
            advertiser: campaign.creator,
            publisher: ADDRESSES["leader"],
            hostname: None,
            country: None,
            os_name: OperatingSystem::Other,
            event_type: "Click".to_string(),
            payout_amount: Default::default(),
        };

        let query_impression_event = EventAnalytics {
            time: Utc::now(),
            campaign_id: DUMMY_CAMPAIGN.id,
            ad_unit: None,
            ad_slot: None,
            ad_slot_type: None,
            advertiser: campaign.creator,
            publisher: ADDRESSES["leader"],
            hostname: None,
            country: None,
            os_name: OperatingSystem::Other,
            event_type: "Click".to_string(),
            payout_amount: Default::default(),
        };

        let (click_analytics, click_count) = find_analytics(&database.pool, &query_click_event)
            .await
            .expect("should find analytics");
        let (impression_analytics, impression_count) = find_analytics(&database.pool, &query_impression_event)
            .await
            .expect("should find analytics");
        assert_eq!(click_analytics.event_type, "Click".to_string());
        assert_eq!(
            click_analytics.payout_amount,
            UnifiedNum::from_u64(1_000_000)
        );
        assert_eq!(click_count, 1);

        assert_eq!(impression_analytics.event_type, "Impression".to_string());
        assert_eq!(
            impression_analytics.payout_amount,
            UnifiedNum::from_u64(1_000_000)
        );
        assert_eq!(impression_count, 1);

        record(&database.clone(), &campaign, &session, input_events)
            .await
            .expect("should record");

        let (click_analytics, click_count) = find_analytics(&database.pool, &query_click_event)
            .await
            .expect("should find analytics");
        let (impression_analytics, impression_count) = find_analytics(&database.pool, &query_impression_event)
            .await
            .expect("should find analytics");
        assert_eq!(
            click_analytics.payout_amount,
            UnifiedNum::from_u64(2_000_000)
        );
        assert_eq!(click_count, 2);

        assert_eq!(
            impression_analytics.payout_amount,
            UnifiedNum::from_u64(2_000_000)
        );
        assert_eq!(impression_count, 2);
    }
}
