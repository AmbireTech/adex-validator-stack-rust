use crate::{
    db::{analytics::update_analytics, DbPool, PoolError},
    Session,
};
use primitives::{
    analytics::OperatingSystem,
    sentry::{DateHour, Event, UpdateAnalytics},
    Address, Campaign, ChainOf, UnifiedNum,
};
use std::collections::HashMap;

/// Validator fees will not be included in analytics
pub async fn record(
    pool: &DbPool,
    campaign_context: &ChainOf<Campaign>,
    session: &Session,
    events_with_payouts: Vec<(Event, Address, UnifiedNum)>,
) -> Result<(), PoolError> {
    let os_name = session
        .os
        .as_ref()
        .map(|os| OperatingSystem::map_os(os))
        .unwrap_or_default();
    // This DateHour is used for all events that are being recorded
    let datehour = DateHour::now();

    let mut batch_update = HashMap::<Event, UpdateAnalytics>::new();

    for (event, _payout_addr, payout_amount) in events_with_payouts {
        let event_type = event.event_type();
        let (publisher, ad_unit, referrer, ad_slot, ad_slot_type) = {
            let (publisher, event_ad_unit, referrer, ad_slot) = match &event {
                Event::Impression {
                    publisher,
                    ad_unit,
                    referrer,
                    ad_slot,
                } => (*publisher, *ad_unit, referrer.clone(), *ad_slot),
                Event::Click {
                    publisher,
                    ad_unit,
                    referrer,
                    ad_slot,
                } => (*publisher, *ad_unit, referrer.clone(), *ad_slot),
            };
            let ad_unit = campaign_context
                .context
                .ad_units
                .iter()
                .find(|ad_unit| ad_unit.ipfs == event_ad_unit);

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

        batch_update
            .entry(event)
            .and_modify(|analytics| {
                analytics.amount_to_add += &payout_amount;
                analytics.count_to_add += 1;
            })
            .or_insert_with(|| UpdateAnalytics {
                campaign_id: campaign_context.context.id,
                time: datehour,
                ad_unit,
                ad_slot,
                ad_slot_type,
                advertiser: campaign_context.context.creator,
                publisher,
                hostname,
                country: session.country.to_owned(),
                os_name: os_name.clone(),
                chain_id: campaign_context.chain.chain_id,
                event_type,
                amount_to_add: payout_amount,
                count_to_add: 1,
            });
    }

    for (_event, update) in batch_update.into_iter() {
        update_analytics(pool, update).await?;
    }
    Ok(())
}

#[cfg(test)]
mod test {
    use super::*;
    use primitives::{
        sentry::{Analytics, CLICK, IMPRESSION},
        test_util::{DUMMY_CAMPAIGN, DUMMY_IPFS, PUBLISHER},
        UnifiedNum,
    };
    use crate::test_util::setup_dummy_app;

    use crate::db::tests_postgres::{setup_test_migrations, DATABASE_POOL};

    // Currently used for testing
    async fn get_all_analytics(pool: &DbPool) -> Result<Vec<Analytics>, PoolError> {
        let client = pool.get().await?;

        let query = "SELECT * FROM analytics";
        let stmt = client.prepare(query).await?;

        let rows = client.query(&stmt, &[]).await?;

        let event_analytics: Vec<Analytics> = rows.iter().map(Analytics::from).collect();
        Ok(event_analytics)
    }

    fn get_test_events() -> HashMap<String, (Event, Address, UnifiedNum)> {
        vec![
            (
                "click".into(),
                (
                    Event::Click {
                        publisher: *PUBLISHER,
                        ad_unit: DUMMY_IPFS[0],
                        ad_slot: DUMMY_IPFS[1],
                        referrer: Some("http://127.0.0.1".into()),
                    },
                    *PUBLISHER,
                    UnifiedNum::from_u64(1_000_000),
                ),
            ),
            (
                "click_with_different_data".into(),
                (
                    Event::Click {
                        publisher: *PUBLISHER,
                        ad_unit: DUMMY_IPFS[2],
                        ad_slot: DUMMY_IPFS[3],
                        referrer: Some("http://127.0.0.1".into()),
                    },
                    *PUBLISHER,
                    UnifiedNum::from_u64(1_000_000),
                ),
            ),
            (
                "impression".into(),
                (
                    Event::Impression {
                        publisher: *PUBLISHER,
                        ad_unit: DUMMY_IPFS[0],
                        ad_slot: DUMMY_IPFS[1],
                        referrer: Some("http://127.0.0.1".into()),
                    },
                    *PUBLISHER,
                    UnifiedNum::from_u64(1_000_000),
                ),
            ),
        ]
        .into_iter()
        .collect::<HashMap<String, _>>()
    }

    #[tokio::test]
    async fn test_analytics_recording_with_empty_events() {
        let app = setup_dummy_app().await;

        let test_events = get_test_events();
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

        let input_events = vec![
            test_events["click"].clone(),
            test_events["impression"].clone(),
            test_events["impression"].clone(),
        ];

        let dummy_channel = DUMMY_CAMPAIGN.channel;
        let channel_chain = app
            .config
            .find_chain_of(dummy_channel.token)
            .expect("Channel token should be whitelisted in config!");
        let channel_context = channel_chain.with_channel(dummy_channel);
        let campaign_context = channel_context.clone().with(campaign);

        record(&database.clone(), &campaign_context, &session, input_events.clone())
            .await
            .expect("should record");

        let analytics = get_all_analytics(&database.pool)
            .await
            .expect("should get all analytics");
        assert_eq!(analytics.len(), 2);

        let click_analytics = analytics
            .iter()
            .find(|a| a.event_type == CLICK)
            .expect("There should be a click Analytics");
        let impression_analytics = analytics
            .iter()
            .find(|a| a.event_type == IMPRESSION)
            .expect("There should be an impression Analytics");
        assert_eq!(
            click_analytics.payout_amount,
            UnifiedNum::from_u64(1_000_000)
        );
        assert_eq!(click_analytics.payout_count, 1);

        assert_eq!(
            impression_analytics.payout_amount,
            UnifiedNum::from_u64(2_000_000)
        );
        assert_eq!(impression_analytics.payout_count, 2);
    }

    #[tokio::test]
    async fn test_recording_with_session() {
        let app = setup_dummy_app().await;

        let database = DATABASE_POOL.get().await.expect("Should get a DB pool");

        setup_test_migrations(database.pool.clone())
            .await
            .expect("Migrations should succeed");

        let test_events = get_test_events();

        let campaign = DUMMY_CAMPAIGN.clone();

        let session = Session {
            ip: Default::default(),
            country: Some("Bulgaria".into()),
            referrer_header: Some("http://127.0.0.1".into()),
            os: Some("Windows".into()),
        };

        let input_events = vec![
            test_events["click"].clone(),
            test_events["click"].clone(),
            test_events["click_with_different_data"].clone(),
            test_events["click_with_different_data"].clone(),
            test_events["click_with_different_data"].clone(),
            test_events["impression"].clone(),
            test_events["impression"].clone(),
            test_events["impression"].clone(),
            test_events["impression"].clone(),
        ];

        let dummy_channel = DUMMY_CAMPAIGN.channel;
        let channel_chain = app
            .config
            .find_chain_of(dummy_channel.token)
            .expect("Channel token should be whitelisted in config!");
        let channel_context = channel_chain.with_channel(dummy_channel);
        let campaign_context = channel_context.clone().with(campaign);
        record(&database.clone(), &campaign_context, &session, input_events.clone())
            .await
            .expect("should record");

        let analytics = get_all_analytics(&database.pool)
            .await
            .expect("should find analytics");

        assert!(
            analytics
                .iter()
                .all(|a| a.os_name == OperatingSystem::map_os("Windows")),
            "all analytics should have the same os as the one in the session"
        );

        let with_slot_and_unit: Analytics = analytics
            .iter()
            .find(|a| {
                a.ad_unit == DUMMY_IPFS[0] && a.ad_slot == DUMMY_IPFS[1] && a.event_type == CLICK
            })
            .expect("entry should exist")
            .to_owned();
        assert_eq!(with_slot_and_unit.hostname, Some("127.0.0.1".to_string()));
        assert_eq!(with_slot_and_unit.payout_count, 2);
        assert_eq!(
            with_slot_and_unit.payout_amount,
            UnifiedNum::from_u64(2_000_000)
        );

        let with_different_slot_and_unit: Analytics = analytics
            .iter()
            .find(|a| a.ad_unit == DUMMY_IPFS[2] && a.ad_slot == DUMMY_IPFS[3])
            .expect("entry should exist")
            .to_owned();
        assert_eq!(with_different_slot_and_unit.payout_count, 3);
        assert_eq!(
            with_different_slot_and_unit.payout_amount,
            UnifiedNum::from_u64(3_000_000)
        );

        let with_referrer: Analytics = analytics
            .iter()
            .find(|a| {
                a.ad_unit == DUMMY_IPFS[0]
                    && a.ad_slot == DUMMY_IPFS[1]
                    && a.event_type == IMPRESSION
            })
            .expect("entry should exist")
            .to_owned();
        assert_eq!(with_referrer.payout_count, 4);
        assert_eq!(with_referrer.payout_amount, UnifiedNum::from_u64(4_000_000));
    }
}
