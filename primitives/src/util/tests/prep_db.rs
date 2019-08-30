use crate::{BigNum, Channel, ChannelSpec, EventSubmission, SpecValidators, ValidatorDesc};
use chrono::{TimeZone, Utc};
use fake::faker::{Faker, Number};
use lazy_static::lazy_static;
use std::collections::HashMap;

lazy_static! {
    static ref IDS: HashMap<&'static str, String> = {
        let mut ids = HashMap::new();
        ids.insert("leader", "awesomeLeader".into());
        ids.insert("follower", "awesomeFollower".into());
        ids.insert("user", "awesomeTestUser".into());
        ids.insert("publisher", "myAwesomePublisher".into());
        ids.insert("publisher2", "myAwesomePublisher2".into());
        ids.insert("creator", "awesomeCreator".into());

        ids
    };

    static ref DUMMY_CHANNEL: Channel = {
        let leader = ValidatorDesc {
            id: "awesomeLeader".to_string(),
            url: "http://localhost:8005".to_string(),
            fee: 100.into(),
        };
        let follower = ValidatorDesc {
            id: "awesomeFollower".to_string(),
            url: "http://localhost:8006".to_string(),
            fee: 100.into(),
        };
        let nonce = BigNum::from(<Faker as Number>::between(100_000_000, 999_999_999));

        Channel {
            id: "awesomeTestChannel".to_string(),
            creator: "awesomeCreator".to_string(),
            deposit_asset: "DAI".to_string(),
            deposit_amount: 1_000.into(),
            // UNIX timestamp for 2100-01-01
            valid_until: Utc.timestamp(4_102_444_800, 0),
            spec: ChannelSpec {
                title: None,
                validators: SpecValidators::new(leader, follower),
                max_per_impression: 10.into(),
                min_per_impression: 10.into(),
                targeting: vec![],
                min_targeting_score: None,
                event_submission: EventSubmission { allow: vec![] },
                // July 29, 2019 7:00:00 AM
                created: Utc.timestamp(1_564_383_600, 0),
                active_from: None,
                nonce,
                withdraw_period_start: Utc.timestamp_millis(4_073_414_400_000),
                ad_units: vec![],
            },
        }
    };
}
