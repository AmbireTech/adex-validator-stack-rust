use redis::aio::MultiplexedConnection;
use crate::Session;
use primitives::{Channel, BigNum};
use primitives::sentry::{Event};
use redis;
use crate::{ epoch };



fn get_payout(channel: &Channel, event: &Event) -> BigNum {
    match event {
        Event::Impression { .. } => channel.spec.min_per_impression,
        Event::Click { .. } => {
            if let Some(pricing) = channel.spec.pricing_bounds {
                pricing.click.min 
            } else {
                BigNum::from(0)
            }
        }
    }
}

pub async fn record(redis: MultiplexedConnection, channel: &Channel, session: &Session, events: &[Event]){
    let db = redis::pipe();

    events.iter().filter(|&ev| ev.is_click_event() || ev.is_impression_event())
        .for_each(|event: &Event| {
            match event {
                Event::Impression { publisher, ad_unit, ad_slot, referrer } | Event::Click { publisher, ad_unit, ad_slot, referrer } => {

                    let payout = get_payout(channel, event).to_u64().expect("should always have a payout");
                    let payAmount = payout / 10u64.pow(18);

                    if let Some(ad_unit) = ad_unit {
                        db.zincr(format!("reportPublisherToAdUnit:{}:{}", event, publisher), ad_unit, 1).ignore();
                        db.zincr(format!("reportChannelToAdUnit:{}:{}", event, publisher), ad_unit, 1).ignore();
                    }

                    if let Some(ad_slot) = ad_slot {
                        db.zincr(format!("reportPublisherToAdSlot:{}:{}", event, publisher), ad_slot, 1).ignore();
                        db.zincr(format!("reportPublisherToAdSlotPay:{}:{}", event, publisher), ad_slot, payout).ignore();
                        
                    }

                    if let Some(country) = session.country {
                        db.zincr(format!("reportPublisherToCountry:{}:{}:{}", epoch().floor(), event, publisher), country, 1).ignore();
                    }

                    let hostname = match(referrer, &session.referrer_header) {
                        (Some(referrer), _) | (_, Some(referrer)) => referrer.split("/").nth(2).map(ToString::to_string)
                    };

                    if let Some(hostname) = &hostname {
                        db.zincr(format!("reportPublisherToHostname:{}:{}", event, publisher), hostname, 1).ignore();
                        db.zincr(format!("reportPublisherToAdSlot:{}:{}", event, channel.id), hostname, 1).ignore();
                        db.zincr(format!("reportPublisherToAdSlot:{}:{}", event, channel.id), hostname, 1).ignore();
                    }
                },
                _ => {}
            }
    });

    if let Err(e) = db.query_async::<_, Option<String>>(&mut redis).await {
        // log error
    }


}