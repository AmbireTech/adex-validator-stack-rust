use crate::{payout::get_payout, Session};
use primitives::{
    sentry::{AggregateEvents, Event, EventAggregate},
    BigNum, Channel, ValidatorId,
};
use slog::Logger;

pub(crate) fn reduce(
    logger: &Logger,
    channel: &Channel,
    initial_aggr: &mut EventAggregate,
    ev: &Event,
    session: &Session,
) -> Result<(), Box<dyn std::error::Error>> {
    let event_type = ev.to_string();
    match ev {
        Event::Impression { publisher, .. } => {
            let impression = initial_aggr.events.get(&event_type);
            let payout = get_payout(logger, &channel, &ev, session)?;
            let merge = merge_payable_event(
                impression,
                payout.unwrap_or_else(|| (*publisher, Default::default())),
            );

            initial_aggr.events.insert(event_type, merge);
        }
        Event::Click { publisher, .. } => {
            let clicks = initial_aggr.events.get(&event_type);
            let payout = get_payout(logger, &channel, &ev, session)?;
            let merge = merge_payable_event(
                clicks,
                payout.unwrap_or_else(|| (*publisher, Default::default())),
            );

            initial_aggr.events.insert(event_type, merge);
        }
        Event::Close => {
            let close_event = AggregateEvents {
                event_counts: Some(vec![(channel.creator, 1.into())].into_iter().collect()),
                event_payouts: vec![(channel.creator, channel.deposit_amount.clone())]
                    .into_iter()
                    .collect(),
            };
            initial_aggr.events.insert(event_type, close_event);
        }
        _ => {}
    };

    Ok(())
}

/// payable_event is either an IMPRESSION or a CLICK
fn merge_payable_event(
    payable_event: Option<&AggregateEvents>,
    payout: (ValidatorId, BigNum),
) -> AggregateEvents {
    let mut payable_event = payable_event.cloned().unwrap_or_default();

    let event_count = payable_event
        .event_counts
        .get_or_insert_with(Default::default)
        .entry(payout.0)
        .or_insert_with(|| 0.into());

    *event_count += &1.into();

    let event_payouts = payable_event
        .event_payouts
        .entry(payout.0)
        .or_insert_with(|| 0.into());
    *event_payouts += &payout.1;

    payable_event
}

#[cfg(test)]
mod test {
    use super::*;
    use chrono::Utc;
    use primitives::util::tests::{
        discard_logger,
        prep_db::{DUMMY_CHANNEL, IDS},
    };
    use primitives::BigNum;

    #[test]
    fn test_reduce() {
        let logger = discard_logger();
        let mut channel: Channel = DUMMY_CHANNEL.clone();
        channel.deposit_amount = 100.into();
        // make immutable again
        let channel = channel;

        let mut event_aggr = EventAggregate {
            channel_id: channel.id,
            created: Utc::now(),
            events: Default::default(),
        };

        let event = Event::Impression {
            publisher: IDS["publisher"],
            ad_unit: None,
            ad_slot: None,
            referrer: None,
        };

        let session = Session {
            ip: Default::default(),
            country: None,
            referrer_header: None,
            os: None,
        };

        for i in 0..101 {
            reduce(&logger, &channel, &mut event_aggr, &event, &session)
                .expect(&format!("Should be able to reduce event #{}", i));
        }

        assert_eq!(event_aggr.channel_id, channel.id);

        let impression_event = event_aggr
            .events
            .get("IMPRESSION")
            .expect("Should have an Impression event");

        let event_counts = impression_event
            .event_counts
            .as_ref()
            .expect("there should be event_counts set")
            .get(&IDS["publisher"])
            .expect("There should be myAwesomePublisher event_counts key");
        assert_eq!(event_counts, &BigNum::from(101));

        let event_payouts = impression_event
            .event_counts
            .as_ref()
            .expect("there should be event_counts set")
            .get(&IDS["publisher"])
            .expect("There should be myAwesomePublisher event_payouts key");
        assert_eq!(event_payouts, &BigNum::from(101));
    }
}
