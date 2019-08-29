use chrono::{DateTime, TimeZone, Utc};
use fake::faker::*;
use time::Duration;

use crate::targeting_tag::fixtures::get_targeting_tags;
use crate::validator::fixtures::get_validator;
use crate::{BigNum, EventSubmission};

use super::{Channel, ChannelSpec, SpecValidators, ValidatorDesc};
use crate::util::tests::take_one;

const ASSETS_LIST: [&str; 8] = ["DAI", "BGN", "EUR", "USD", "ADX", "BTC", "LIT", "ETH"];

///// It will get the length of channel_id bytes and will fill enough bytes in front
///// If > 32 bytes &str is passed it will `panic!`
//pub fn get_channel_id(channel_id: &str) -> ChannelId {
//    let channel_id_bytes = channel_id.as_bytes();
//    if channel_id_bytes.len() > 32 {
//        panic!("The passed &str should be <= 32 bytes");
//    }
//
//    let mut id: [u8; 32] = [b'0'; 32];
//    for (index, byte) in id[32 - channel_id.len()..].iter_mut().enumerate() {
//        *byte = channel_id_bytes[index];
//    }
//
//    ChannelId { bytes: id }
//}

pub fn get_dummy_channel() -> Channel {
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
            validators: SpecValidators([leader, follower]),
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
}

pub fn get_channel(
    id: &str,
    valid_until: &Option<DateTime<Utc>>,
    spec: Option<ChannelSpec>,
) -> Channel {
    let deposit_amount = BigNum::from(<Faker as Number>::between(100, 5000));
    let valid_until: DateTime<Utc> = valid_until.unwrap_or_else(|| {
        let future_from = Utc::now() + Duration::days(7);
        crate::util::tests::time::datetime_between(&future_from, None)
    });
    let creator = <Faker as Name>::name();
    let deposit_asset = take_one(&ASSETS_LIST).into();
    let spec = spec.unwrap_or_else(|| {
        get_channel_spec(ValidatorsOption::Generate {
            validators_prefix: id,
        })
    });

    Channel {
        id: id.into(),
        creator,
        deposit_asset,
        deposit_amount,
        valid_until,
        spec,
    }
}
//
//pub fn get_channels(count: usize, valid_until_ge: Option<DateTime<Utc>>) -> Vec<Channel> {
//    (1..=count)
//        .map(|c| {
//            // if we have a valid_until_ge, use it to generate a valid_util for each channel
//            let valid_until =
//                valid_until_ge.and_then(|ref dt| Some(test_util::time::datetime_between(dt, None)));
//            let channel_id = format!("channel {}", c);
//
//            get_channel(&channel_id, &valid_until, None)
//        })
//        .collect()
//}
//
pub enum ValidatorsOption<'a> {
    Pair {
        leader: ValidatorDesc,
        follower: ValidatorDesc,
    },
    SpecValidators(SpecValidators),
    Generate {
        validators_prefix: &'a str,
    },
}

pub fn get_channel_spec(validators_option: ValidatorsOption<'_>) -> ChannelSpec {
    let validators = match validators_option {
        ValidatorsOption::Pair { leader, follower } => [leader, follower].into(),
        ValidatorsOption::SpecValidators(spec_validators) => spec_validators,
        ValidatorsOption::Generate { validators_prefix } => [
            get_validator(&format!("{} leader", validators_prefix), None),
            get_validator(&format!("{} follower", validators_prefix), None),
        ]
        .into(),
    };

    let title_string = Some(<Faker as Lorem>::sentence(3, 4));

    let title = take_one(&[&title_string, &None]).to_owned();
    let max_per_impression = BigNum::from(<Faker as Number>::between(250, 500));
    let min_per_impression = BigNum::from(<Faker as Number>::between(1, 250));
    let nonce = BigNum::from(<Faker as Number>::between(100_000_000, 999_999_999));
    let min_targeting_score =
        take_one(&[&None, &Some(<Faker as Number>::between(1_f64, 500_f64))]).to_owned();

    ChannelSpec {
        validators,
        title,
        max_per_impression,
        min_per_impression,
        targeting: get_targeting_tags(<Faker as Number>::between(0, 5)),
        min_targeting_score,
        // @TODO: `EventSubmission` fixture issue #27
        event_submission: EventSubmission { allow: vec![] },
        created: Utc::now(),
        active_from: Some(Utc::now()),
        nonce,
        withdraw_period_start: Utc::now(),
        ad_units: Vec::new(),
    }
}
