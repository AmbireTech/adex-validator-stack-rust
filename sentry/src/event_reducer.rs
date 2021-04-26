use primitives::{
    sentry::{AggregateEvents, Event, EventAggregate},
    Address, BigNum, Channel,
};

//
// TODO: AIP#61 remove `allow(dead_code)` and see what should be changed for Spender Aggregate
//
#[allow(dead_code, clippy::unnecessary_wraps)]
pub(crate) fn reduce(
    channel: &Channel,
    initial_aggr: &mut EventAggregate,
    ev: &Event,
    payout: &Option<(Address, BigNum)>,
) -> Result<(), Box<dyn std::error::Error>> {
    let event_type = ev.to_string();

    match ev {
        Event::Impression { publisher, .. } => {
            let impression = initial_aggr.events.get(&event_type);
            let merge = merge_payable_event(
                impression,
                payout
                    .to_owned()
                    .unwrap_or_else(|| (*publisher, Default::default())),
            );

            initial_aggr.events.insert(event_type, merge);
        }
        Event::Click { publisher, .. } => {
            let clicks = initial_aggr.events.get(&event_type);
            let merge = merge_payable_event(
                clicks,
                payout
                    .to_owned()
                    .unwrap_or_else(|| (*publisher, Default::default())),
            );

            initial_aggr.events.insert(event_type, merge);
        }
        Event::Close => {
            let close_event = AggregateEvents {
                event_counts: Some(
                    vec![(channel.creator.to_address(), 1.into())]
                        .into_iter()
                        .collect(),
                ),
                event_payouts: vec![(channel.creator.to_address(), channel.deposit_amount.clone())]
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
    payout: (Address, BigNum),
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
    use primitives::{
        util::tests::prep_db::{ADDRESSES, DUMMY_CHANNEL},
        BigNum,
    };

    #[test]
    fn test_reduce() {
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
            publisher: ADDRESSES["publisher"],
            ad_unit: None,
            ad_slot: None,
            referrer: None,
        };
        let payout = Some((ADDRESSES["publisher"], BigNum::from(1)));
        for i in 0..101 {
            reduce(&channel, &mut event_aggr, &event, &payout)
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
            .get(&ADDRESSES["publisher"])
            .expect("There should be myAwesomePublisher event_counts key");
        assert_eq!(event_counts, &BigNum::from(101));

        let event_payouts = impression_event
            .event_payouts
            .get(&ADDRESSES["publisher"])
            .expect("There should be myAwesomePublisher event_payouts key");
        assert_eq!(event_payouts, &BigNum::from(101));
    }
}
