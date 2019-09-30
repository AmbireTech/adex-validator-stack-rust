use crate::{BigNum, Channel, ChannelSpec, EventSubmission, SpecValidators, ValidatorDesc};
use chrono::{TimeZone, Utc};
use fake::faker::{Faker, Number};
use lazy_static::lazy_static;
use std::collections::HashMap;

lazy_static! {
    pub static ref IDS: HashMap<&'static str, String> = {
        let mut ids = HashMap::new();

        ids.insert("leader", "awesomeLeader".into());
        ids.insert("follower", "awesomeFollower".into());
        ids.insert("user", "awesomeTestUser".into());
        ids.insert("publisher", "b7d3f81e857692d13e9d63b232a90f4a1793189e".into());
        ids.insert("publisher2", "myAwesomePublisher2".into());
        ids.insert("creator", "awesomeCreator".into());
        ids.insert("tester", "2892f6C41E0718eeeDd49D98D648C789668cA67d".into());

        ids
    };

    pub static ref AUTH: HashMap<&'static str, String> = {
        let mut auth = HashMap::new();

        auth.insert("leader", "AUTH_awesomeLeader".into());
        auth.insert("follower", "AUTH_awesomeLeader".into());
        auth.insert("user", "x8c9v1b2".into());
        auth.insert("publisher", "testing".into());
        auth.insert("publisher2", "testing2".into());
        auth.insert("creator", "awesomeCreator".into());
        auth.insert("tester", "AUTH_awesomeTester".into());

        auth
    };

    pub static ref DUMMY_VALIDATOR_LEADER: ValidatorDesc = ValidatorDesc {
        id: "awesomeLeader".to_string(),
        url: "http://localhost:8005".to_string(),
        fee: 100.into(),
    };

    pub static ref DUMMY_VALIDATOR_FOLLOWER: ValidatorDesc = ValidatorDesc {
        id: "awesomeFollower".to_string(),
        url: "http://localhost:8006".to_string(),
        fee: 100.into(),
    };

    pub static ref DUMMY_CHANNEL: Channel = {
        let nonce = BigNum::from(<Faker as Number>::between(100_000_000, 999_999_999));

        Channel {
            id: "061d5e2a67d0a9a10f1c732bca12a676d83f79663a396f7d87b3e30b9b411088".to_string(),
            creator: "awesomeCreator".to_string(),
            deposit_asset: "DAI".to_string(),
            deposit_amount: 1_000.into(),
            // UNIX timestamp for 2100-01-01
            valid_until: Utc.timestamp(4_102_444_800, 0),
            spec: ChannelSpec {
                title: None,
                validators: SpecValidators::new(DUMMY_VALIDATOR_LEADER.clone(), DUMMY_VALIDATOR_FOLLOWER.clone()),
                max_per_impression: 10.into(),
                min_per_impression: 10.into(),
                targeting: vec![],
                min_targeting_score: None,
                event_submission: Some(EventSubmission { allow: vec![] }),
                // July 29, 2019 7:00:00 AM
                created: Some(Utc.timestamp(1_564_383_600, 0)),
                active_from: None,
                nonce: Some(nonce),
                withdraw_period_start: Utc.timestamp_millis(4_073_414_400_000),
                ad_units: vec![],
            },
        }
    };
}
