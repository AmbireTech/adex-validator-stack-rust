use crate::{
    channel::{Pricing, PricingBounds},
    targeting::Rules,
    AdUnit, BigNum, Channel, ChannelId, ChannelSpec, EventSubmission, SpecValidators,
    ValidatorDesc, ValidatorId, IPFS,
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
            targeting_rules: Rules::new(),
            // UNIX timestamp for 2100-01-01
            valid_until: Utc.timestamp(4_102_444_800, 0),
            spec: ChannelSpec {
                title: None,
                validators: SpecValidators::new(DUMMY_VALIDATOR_LEADER.clone(), DUMMY_VALIDATOR_FOLLOWER.clone()),
                max_per_impression: 10.into(),
                min_per_impression: 1.into(),
                targeting_rules: Rules::new(),
                event_submission: Some(EventSubmission { allow: vec![] }),
                // July 29, 2019 7:00:00 AM
                created: Utc.timestamp(1_564_383_600, 0),
                active_from: None,
                nonce: Some(nonce),
                withdraw_period_start: Utc.timestamp_millis(4_073_414_400_000),
                ad_units: vec![],
                pricing_bounds: Some(PricingBounds {impression: None, click: Some(Pricing { max: 0.into(), min: 0.into()})}),
            },
            exhausted: Default::default(),
        }
    };

    pub static ref DUMMY_AD_UNITS: [AdUnit; 4] = [
        AdUnit {
            ipfs: IPFS::try_from("Qmasg8FrbuSQpjFu3kRnZF9beg8rEBFrqgi1uXDRwCbX5f")
                .expect("should convert"),
            media_url: "ipfs://QmcUVX7fvoLMM93uN2bD3wGTH8MXSxeL8hojYfL2Lhp7mR".to_string(),
            media_mime: "image/jpeg".to_string(),
            target_url: "https://www.adex.network/?stremio-test-banner-1".to_string(),
            archived: false,
            description: Some("Dummy AdUnit description 1".to_string()),
            ad_type: "legacy_250x250".to_string(),
            /// Timestamp: 1 564 383 600
            created: Utc.ymd(2019, 7, 29).and_hms(9, 0, 0),
            min_targeting_score: None,
            modified: None,
            owner: IDS["publisher"],
            title: Some("Dummy AdUnit 3".to_string()),
        },
        AdUnit {
            ipfs: IPFS::try_from("QmVhRDGXoM3Fg3HZD5xwMuxtb9ZErwC8wHt8CjsfxaiUbZ")
                .expect("should convert"),
            media_url: "ipfs://QmQB7uz7Gxfy7wqAnrnBcZFaVJLos8J9gn8mRcHQU6dAi1".to_string(),
            media_mime: "image/jpeg".to_string(),
            target_url: "https://www.adex.network/?adex-campaign=true&pub=stremio".to_string(),
            archived: false,
            description: Some("Dummy AdUnit description 2".to_string()),
            ad_type: "legacy_250x250".to_string(),
            /// Timestamp: 1 564 383 600
            created: Utc.ymd(2019, 7, 29).and_hms(9, 0, 0),
            min_targeting_score: None,
            modified: None,
            owner: IDS["publisher"],
            title: Some("Dummy AdUnit 2".to_string()),
        },
        AdUnit {
            ipfs: IPFS::try_from("QmYwcpMjmqJfo9ot1jGe9rfXsszFV1WbEA59QS7dEVHfJi")
                .expect("should convert"),
            media_url: "ipfs://QmQB7uz7Gxfy7wqAnrnBcZFaVJLos8J9gn8mRcHQU6dAi1".to_string(),
            media_mime: "image/jpeg".to_string(),
            target_url: "https://www.adex.network/?adex-campaign=true".to_string(),
            archived: false,
            description: Some("Dummy AdUnit description 3".to_string()),
            ad_type: "legacy_250x250".to_string(),
            /// Timestamp: 1 564 383 600
            created: Utc.ymd(2019, 7, 29).and_hms(9, 0, 0),
            min_targeting_score: None,
            modified: None,
            owner: IDS["publisher"],
            title: Some("Dummy AdUnit 3".to_string()),
        },
        AdUnit {
            ipfs: IPFS::try_from("QmTAF3FsFDS7Ru8WChoD9ofiHTH8gAQfR4mYSnwxqTDpJH")
                .expect("should convert"),
            media_url: "ipfs://QmQAcfBJpDDuH99A4p3pFtUmQwamS8UYStP5HxHC7bgYXY".to_string(),
            media_mime: "image/jpeg".to_string(),
            target_url: "https://adex.network".to_string(),
            archived: false,
            description: Some("Dummy AdUnit description 4".to_string()),
            ad_type: "legacy_250x250".to_string(),
            /// Timestamp: 1 564 383 600
            created: Utc.ymd(2019, 7, 29).and_hms(9, 0, 0),
            min_targeting_score: None,
            modified: None,
            owner: IDS["publisher"],
            title: Some("Dummy AdUnit 4".to_string()),
        },
    ];

    // CID V0
    pub static ref DUMMY_IPFS: [IPFS; 5] = [
        IPFS::try_from("QmcUVX7fvoLMM93uN2bD3wGTH8MXSxeL8hojYfL2Lhp7mR").expect("Valid IPFS V0"),
        IPFS::try_from("Qmasg8FrbuSQpjFu3kRnZF9beg8rEBFrqgi1uXDRwCbX5f").expect("Valid IPFS V0"),
        IPFS::try_from("QmQnu8zrHsuVvnTJsEgDHYA8c1MmRL7YLiMD8uzDUJKcNq").expect("Valid IPFS V0"),
        IPFS::try_from("QmYYBULc9QDEaDr8HAXvVWHDmFfL2GvyumYRr1g4ERBC96").expect("Valid IPFS V0"),
        // V1 of the V0 ipfs: `QmcUVX7fvoLMM93uN2bD3wGTH8MXSxeL8hojYfL2Lhp7mR`
        IPFS::try_from("bafybeif2h3mynaf3ylgdbs6arf6mczqycargt5cqm3rmel3wpjarlswway").expect("Valid IPFS V1"),
    ];

}
