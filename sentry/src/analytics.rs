use crate::{
    db::{analytics::insert_analytics, DbPool, PoolError},
    Session,
};
use chrono::Utc;
use primitives::{
    analytics::map_os,
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
    let os_name = map_os(session.os.as_ref().unwrap_or(&"".to_string()));
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
            time,
            campaign_id: campaign.id,
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

        insert_analytics(pool, event_for_db).await?;
    }

    Ok(())
}
