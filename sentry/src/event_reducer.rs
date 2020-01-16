use primitives::sentry::{AggregateEvents, Event, EventAggregate};
use primitives::Channel;

// @TODO: Remove attribute once we use this function!
#[allow(dead_code)]
pub(crate) fn reduce(channel: &Channel, initial_aggr: &mut EventAggregate, ev: &Event) {
    match ev {
        Event::Impression { publisher, .. } => {
            let impression = initial_aggr.events.get("IMPRESSION");

            let merge = merge_impression_ev(impression, &publisher, &channel);

            initial_aggr.events.insert("IMPRESSION".to_owned(), merge);
        }
        Event::Close => {
            let creator = channel.creator.clone();
            let close_event = AggregateEvents {
                event_counts: Some(vec![(creator.clone(), 1.into())].into_iter().collect()),
                event_payouts: vec![(creator, channel.deposit_amount.clone())]
                    .into_iter()
                    .collect(),
            };
            initial_aggr.events.insert("CLOSE".to_owned(), close_event);
        }
        _ => {}
    };
}

fn merge_impression_ev(
    impression: Option<&AggregateEvents>,
    earner: &ValidatorId,
    channel: &Channel,
) -> AggregateEvents {
    let mut impression = impression.map(Clone::clone).unwrap_or_default();

    let event_count = impression
        .event_counts
        .get_or_insert_with(Default::default)
        .entry(earner.clone())
        .or_insert_with(|| 0.into());

    *event_count += &1.into();

    let event_payouts = impression
        .event_payouts
        .entry(earner.clone())
        .or_insert_with(|| 0.into());
    *event_payouts += &channel.spec.min_per_impression;

    impression
}

#[cfg(test)]
mod test {
    use super::*;
    use chrono::Utc;
    use primitives::util::tests::prep_db::{DUMMY_CHANNEL, IDS};
    use primitives::BigNum;

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
            publisher: IDS["publisher"].clone(),
            ad_unit: None,
        };

        for _ in 0..101 {
            reduce(&channel, &mut event_aggr, &event);
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
