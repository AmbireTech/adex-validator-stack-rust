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
                event_counts: vec![(creator.clone(), 1.into())].into_iter().collect(),
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
    earner: &str,
    channel: &Channel,
) -> AggregateEvents {
    let mut impression = impression
        .map(Clone::clone)
        .unwrap_or_else(Default::default);

    let event_counts = impression
        .event_counts
        .entry(earner.into())
        .or_insert_with(|| 0.into());
    *event_counts += &1.into();

    let event_payouts = impression
        .event_payouts
        .entry(earner.into())
        .or_insert_with(|| 0.into());
    *event_payouts += &channel.spec.min_per_impression;

    impression
}

#[cfg(test)]
mod test {
    use super::*;
    use chrono::Utc;
    use primitives::util::tests::prep_db::DUMMY_CHANNEL;
    use primitives::BigNum;

    #[test]
    fn test_reduce() {
        let mut channel: Channel = DUMMY_CHANNEL.clone();
        channel.deposit_amount = 100.into();
        // make immutable again
        let channel = channel;

        let mut event_aggr = EventAggregate {
            channel_id: channel.id.clone(),
            created: Utc::now(),
            events: Default::default(),
        };

        let event = Event::Impression {
            publisher: "myAwesomePublisher".to_string(),
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
            .get("myAwesomePublisher")
            .expect("There should be myAwesomePublisher event_counts key");
        assert_eq!(event_counts, &BigNum::from(101));

        let event_payouts = impression_event
            .event_counts
            .get("myAwesomePublisher")
            .expect("There should be myAwesomePublisher event_payouts key");
        assert_eq!(event_payouts, &BigNum::from(101));
    }
}
