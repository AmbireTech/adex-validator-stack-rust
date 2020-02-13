use crate::epoch;
use crate::Session;
use primitives::sentry::Event;
use primitives::sentry::{ChannelReport, PublisherReport};
use primitives::{BigNum, Channel};
use redis;
use redis::aio::MultiplexedConnection;
use slog::{error, Logger};

fn get_payout(channel: &Channel, event: &Event) -> BigNum {
    match event {
        Event::Impression { .. } => channel.spec.min_per_impression.clone(),
        Event::Click { .. } => {
            if let Some(pricing) = channel.spec.pricing_bounds.clone() {
                pricing.click.min
            } else {
                BigNum::from(0)
            }
        }
        _ => BigNum::from(0),
    }
}

pub async fn record(
    mut conn: MultiplexedConnection,
    channel: Channel,
    session: Session,
    events: Vec<Event>,
    logger: Logger,
) {
    let mut db = redis::pipe();

    events
        .iter()
        .filter(|&ev| ev.is_click_event() || ev.is_impression_event())
        .for_each(|event: &Event| match event {
            Event::Impression {
                publisher,
                ad_unit,
                ad_slot,
                referrer,
            }
            | Event::Click {
                publisher,
                ad_unit,
                ad_slot,
                referrer,
            } => {
                let divisor = BigNum::from(10u64.pow(18));
                let pay_amount = get_payout(&channel, event)
                    .div_floor(&divisor)
                    .to_u64()
                    .expect("should always have a payout");
                // let pay_amount = payout / 10u64.pow(18);

                if let Some(ad_unit) = ad_unit {
                    db.zincr(
                        format!(
                            "{}:{}:{}",
                            PublisherReport::ReportPublisherToAdUnit,
                            event,
                            publisher
                        ),
                        ad_unit,
                        1,
                    )
                    .ignore();
                    db.zincr(
                        format!(
                            "{}:{}:{}",
                            ChannelReport::ReportChannelToAdUnit,
                            event,
                            publisher
                        ),
                        ad_unit,
                        1,
                    )
                    .ignore();
                }

                if let Some(ad_slot) = ad_slot {
                    db.zincr(
                        format!(
                            "{}:{}:{}",
                            PublisherReport::ReportPublisherToAdSlot,
                            event,
                            publisher
                        ),
                        ad_slot,
                        1,
                    )
                    .ignore();
                    db.zincr(
                        format!(
                            "{}:{}:{}",
                            PublisherReport::ReportPublisherToAdSlotPay,
                            event,
                            publisher
                        ),
                        ad_slot,
                        pay_amount,
                    )
                    .ignore();
                }

                if let Some(country) = session.country.clone() {
                    db.zincr(
                        format!(
                            "{}:{}:{}:{}",
                            PublisherReport::ReportPublisherToCountry,
                            epoch().floor(),
                            event,
                            publisher
                        ),
                        country,
                        1,
                    )
                    .ignore();
                }

                let hostname = match (referrer, &session.referrer_header) {
                    (Some(referrer), _) | (_, Some(referrer)) => {
                        referrer.split('/').nth(2).map(ToString::to_string)
                    }
                    _ => None,
                };

                if let Some(hostname) = &hostname {
                    db.zincr(
                        format!(
                            "{}:{}:{}",
                            PublisherReport::ReportPublisherToHostname,
                            event,
                            publisher
                        ),
                        hostname,
                        1,
                    )
                    .ignore();
                    db.zincr(
                        format!(
                            "{}:{}:{}",
                            ChannelReport::ReportChannelToHostname,
                            event,
                            channel.id
                        ),
                        hostname,
                        1,
                    )
                    .ignore();
                    db.zincr(
                        format!(
                            "{}:{}:{}",
                            ChannelReport::ReportChannelToHostnamePay,
                            event,
                            channel.id
                        ),
                        hostname,
                        1,
                    )
                    .ignore();
                }
            }
            _ => {}
        });

    if let Err(err) = db.query_async::<_, Option<String>>(&mut conn).await {
        error!(&logger, "Server error: {}", err; "module" => "analytics-recorder");
    }
}
