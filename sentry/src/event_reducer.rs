use primitives::sentry::{AggregateEvents, Event, EventAggregate};
use primitives::Channel;

// @TODO: Remove attribute once we use this function!
#[allow(dead_code)]
pub(crate) fn reduce(
    channel: &Channel,
    mut initial_aggr: EventAggregate,
    ev: &Event,
) -> EventAggregate {
    match ev {
        Event::Impression { publisher, .. } => {
            let impression = initial_aggr
                .events
                .get("IMPRESSION")
                .expect("Event IMPRESSION should exist in the EventAggregate");

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

    initial_aggr
}

fn merge_impression_ev(
    impression: &AggregateEvents,
    earner: &str,
    channel: &Channel,
) -> AggregateEvents {
    let mut impression = impression.clone();

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
