use primitives::sentry::{AggregateEvents, Event, EventAggregate};
use primitives::Channel;

pub(crate) fn reduce(
    channel: &Channel,
    mut initial_aggr: EventAggregate,
    ev: &Event,
) -> EventAggregate {
    match ev {
        Event::Impression { .. } => {
            let impression = initial_aggr
                .events
                .get("IMPRESSION")
                .expect("Event IMPRESSION should exist in the EventAggregate");

            let merge = merge_impression_ev(impression, &ev, &channel);

            initial_aggr.events.insert("IMPRESSION".to_owned(), merge)
        }
        Event::Close => {
            let creator = channel.creator.clone();
            let close_event = AggregateEvents {
                event_counts: vec![(creator.clone(), 1.into())].into_iter().collect(),
                event_payouts: vec![(creator, channel.deposit_amount.clone())]
                    .into_iter()
                    .collect(),
            };
            initial_aggr.events.insert("CLOSE".to_owned(), close_event)
        }
        _ => panic!("Whoopsy for now"),
    };

    initial_aggr
}

fn merge_impression_ev(
    _aggr_events: &AggregateEvents,
    _ev: &Event,
    _channel: &Channel,
) -> AggregateEvents {
    unimplemented!();
}
