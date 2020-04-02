use crate::epoch;
use crate::payout::get_payout;
use crate::Session;
use primitives::sentry::Event;
use primitives::sentry::{ChannelReport, PublisherReport};
use primitives::{BigNum, Channel};
use redis::aio::MultiplexedConnection;
use redis::pipe;
use slog::{error, Logger};

pub async fn record(
    mut conn: MultiplexedConnection,
    channel: Channel,
    session: Session,
    events: Vec<Event>,
    logger: Logger,
) {
    let mut db = pipe();

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
                let pay_amount = get_payout(&channel, event, &session)
                    .div_floor(&divisor)
                    .to_f64()
                    .expect("should always have a payout");

                if let Some(ad_unit) = ad_unit {
                    db.zincr(
                        format!("{}:{}:{}", PublisherReport::AdUnit, event, publisher),
                        ad_unit,
                        1,
                    )
                    .ignore();
                    db.zincr(
                        format!("{}:{}:{}", ChannelReport::AdUnit, event, publisher),
                        ad_unit,
                        1,
                    )
                    .ignore();
                }

                if let Some(ad_slot) = ad_slot {
                    db.zincr(
                        format!("{}:{}:{}", PublisherReport::AdSlot, event, publisher),
                        ad_slot,
                        1,
                    )
                    .ignore();
                    db.zincr(
                        format!("{}:{}:{}", PublisherReport::AdSlotPay, event, publisher),
                        ad_slot,
                        pay_amount,
                    )
                    .ignore();
                }

                if let Some(country) = &session.country {
                    db.zincr(
                        format!(
                            "{}:{}:{}:{}",
                            PublisherReport::Country,
                            epoch().floor(),
                            event,
                            publisher
                        ),
                        country,
                        1,
                    )
                    .ignore();
                }

                let hostname = (referrer.as_ref())
                    .or_else(|| session.referrer_header.as_ref())
                    .map(|rf| rf.split('/').nth(2).map(ToString::to_string))
                    .flatten();

                if let Some(hostname) = &hostname {
                    db.zincr(
                        format!("{}:{}:{}", PublisherReport::Hostname, event, publisher),
                        hostname,
                        1,
                    )
                    .ignore();
                    db.zincr(
                        format!("{}:{}:{}", ChannelReport::Hostname, event, channel.id),
                        hostname,
                        1,
                    )
                    .ignore();
                    db.zincr(
                        format!("{}:{}:{}", ChannelReport::HostnamePay, event, channel.id),
                        hostname,
                        1,
                    )
                    .ignore();
                }
            }
            _ => {}
        });

    if let Err(err) = db.query_async::<_, Option<String>>(&mut conn).await {
        error!(&logger, "Redis Database error: {}", err; "module" => "analytics-recorder");
    }
}
