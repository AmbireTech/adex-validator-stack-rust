use std::collections::HashMap;

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
    campaign: Campaign,
    session: Session,
    events_with_payouts: Vec<(Event, HashMap<Address, UnifiedNum>)>,
) -> Result<(), PoolError> {
    let os_name = map_os(&session.os.unwrap_or_default());
    let time = Utc::now();
    let country = session.country;

    for (event, event_payout) in events_with_payouts {
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
                    .clone() // TODO: Remove clone
                    .ad_units
                    .into_iter()
                    .find(|ad_unit| ad_unit.ipfs == ipfs)
            });
            let ad_slot_type = ad_unit.as_ref().map(|unit| unit.ad_type.clone());
            (publisher, ad_unit, referrer, ad_slot, ad_slot_type)
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
            country: country.to_owned(),
            os_name: os_name.to_owned(),
        };
        let payout_amount = event_payout.values().sum::<Option<UnifiedNum>>().unwrap(); // TODO: Remove

        insert_analytics(pool, event_for_db, payout_amount).await?;
    }

    Ok(())
}
