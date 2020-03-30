use crate::{
    channel::{Pricing, PricingBounds},
    BigNum, Channel, ChannelId, ChannelSpec, EventSubmission, SpecValidators, ValidatorDesc,
    ValidatorId,
};
use chrono::{TimeZone, Utc};
use fake::faker::{Faker, Number};
use hex::FromHex;
use lazy_static::lazy_static;
use std::collections::HashMap;
use std::convert::TryFrom;

lazy_static! {
    // dummy auth
    // session_tokens
    pub static ref IDS: HashMap<String, ValidatorId> = {
        let mut ids = HashMap::new();

        ids.insert("leader".into(),  ValidatorId::try_from("0xce07CbB7e054514D590a0262C93070D838bFBA2e").expect("failed to parse id"));
        ids.insert("follower".into(), ValidatorId::try_from("0xc91763d7f14ac5c5ddfbcd012e0d2a61ab9bded3").expect("failed to parse id"));
        ids.insert("user".into(), ValidatorId::try_from("0x20754168c00a6e58116ccfd0a5f7d1bb66c5de9d").expect("failed to parse id"));
        ids.insert("publisher".into(), ValidatorId::try_from("0xb7d3f81e857692d13e9d63b232a90f4a1793189e").expect("failed to parse id"));
        ids.insert("publisher2".into(), ValidatorId::try_from("0x2054b0c1339309597ad04ba47f4590f8cdb4e305").expect("failed to parse id"));
        ids.insert("creator".into(), ValidatorId::try_from("0x033ed90e0fec3f3ea1c9b005c724d704501e0196").expect("failed to parse id"));
        ids.insert("tester".into(), ValidatorId::try_from("0x2892f6C41E0718eeeDd49D98D648C789668cA67d").expect("failed to parse id"));

        ids
    };
    // dummy auth tokens
    // authorization tokens
    pub static ref AUTH: HashMap<String, String> = {
        let mut auth = HashMap::new();

        auth.insert("leader".into(), "AUTH_awesomeLeader".into());
        auth.insert("follower".into(), "AUTH_awesomeFollower".into());
        auth.insert("user".into(), "x8c9v1b2".into());
        auth.insert("publisher".into(), "testing".into());
        auth.insert("publisher2".into(), "testing2".into());
        auth.insert("creator".into(), "0x033Ed90e0FeC3F3ea1C9b005C724D704501e0196".into());
        auth.insert("tester".into(), "AUTH_awesomeTester".into());

        auth
    };

    pub static ref DUMMY_VALIDATOR_LEADER: ValidatorDesc = ValidatorDesc {
        id:  ValidatorId::try_from("ce07CbB7e054514D590a0262C93070D838bFBA2e").expect("Failed to parse DUMMY_VALIDATOR_LEADER id "),
        url: "http://localhost:8005".to_string(),
        fee: 100.into(),
        fee_addr: None,
    };

    pub static ref DUMMY_VALIDATOR_FOLLOWER: ValidatorDesc = ValidatorDesc {
        id:  ValidatorId::try_from("c91763d7f14ac5c5ddfbcd012e0d2a61ab9bded3").expect("Failed to parse DUMMY_VALIDATOR_FOLLOWER id "),
        url: "http://localhost:8006".to_string(),
        fee: 100.into(),
        fee_addr: None,
    };

    pub static ref DUMMY_CHANNEL: Channel = {
        let nonce = BigNum::from(<Faker as Number>::between(100_000_000, 999_999_999));

        Channel {
            id: ChannelId::from_hex("061d5e2a67d0a9a10f1c732bca12a676d83f79663a396f7d87b3e30b9b411088").expect("prep_db: failed to deserialize channel id"),
            creator: ValidatorId::try_from("033ed90e0fec3f3ea1c9b005c724d704501e0196").expect("Should be valid ValidatorId"),
            deposit_asset: "0x89d24A6b4CcB1B6fAA2625fE562bDD9a23260359".to_string(),
            deposit_amount: 1_000.into(),
            // UNIX timestamp for 2100-01-01
            valid_until: Utc.timestamp(4_102_444_800, 0),
            spec: ChannelSpec {
                title: None,
                validators: SpecValidators::new(DUMMY_VALIDATOR_LEADER.clone(), DUMMY_VALIDATOR_FOLLOWER.clone()),
                max_per_impression: 10.into(),
                min_per_impression: 1.into(),
                targeting: vec![],
                min_targeting_score: None,
                event_submission: Some(EventSubmission { allow: vec![] }),
                // July 29, 2019 7:00:00 AM
                created: Some(Utc.timestamp(1_564_383_600, 0)),
                active_from: None,
                nonce: Some(nonce),
                withdraw_period_start: Utc.timestamp_millis(4_073_414_400_000),
                ad_units: vec![],
                pricing_bounds: Some(PricingBounds {impression: None, click: Some(Pricing { max: 0.into(), min: 0.into()})}),
                price_multiplication_rules: Default::default(),
                price_dynamic_adjustment: false,
            },
        }
    };
}
