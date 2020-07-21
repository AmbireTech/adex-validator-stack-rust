use crate::payout::get_payout;
use crate::Session;
use primitives::sentry::{AggregateEvents, Event, EventAggregate};
use primitives::{BigNum, Channel, ValidatorId};

pub(crate) fn reduce(
    channel: &Channel,
    initial_aggr: &mut EventAggregate,
    ev: &Event,
    session: &Session,
) {
    match ev {
        Event::Impression { publisher, .. } => {
            let impression = initial_aggr.events.get("IMPRESSION");
            let payout = get_payout(&channel, &ev, session);
            let merge = merge_impression_ev(impression, &publisher, &payout);

            initial_aggr.events.insert("IMPRESSION".to_owned(), merge);
        }
        Event::Click { publisher, .. } => {
            let clicks = initial_aggr.events.get("CLICK");
            let payout = get_payout(&channel, &ev, session);
            let merge = merge_impression_ev(clicks, &publisher, &payout);

            initial_aggr.events.insert("CLICK".to_owned(), merge);
        }
        Event::Close => {
            let close_event = AggregateEvents {
                event_counts: Some(vec![(channel.creator, 1.into())].into_iter().collect()),
                event_payouts: vec![(channel.creator, channel.deposit_amount.clone())]
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
    payout: &BigNum,
) -> AggregateEvents {
    let mut impression = impression.map(Clone::clone).unwrap_or_default();

    let event_count = impression
        .event_counts
        .get_or_insert_with(Default::default)
        .entry(*earner)
        .or_insert_with(|| 0.into());

    *event_count += &1.into();

    let event_payouts = impression
        .event_payouts
        .entry(*earner)
        .or_insert_with(|| 0.into());
    *event_payouts += payout;

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

        for _ in 0..101 {
            reduce(&channel, &mut event_aggr, &event, &session);
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
