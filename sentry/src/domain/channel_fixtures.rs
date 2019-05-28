use std::convert::TryFrom;

use chrono::{DateTime, Utc};
use fake::faker::*;

use crate::domain::asset::fixtures::get_asset;
use crate::domain::BigNum;
use crate::domain::validator::fixtures::get_validators;
use crate::domain::ValidatorDesc;
use crate::test_util;

use super::{Channel, ChannelId, ChannelSpec};

/// It will get the length of channel_id bytes and will fill enough bytes in front
/// If > 32 bytes &str is passed it will `panic!`
pub fn get_channel_id(channel_id: &str) -> ChannelId {
    let channel_id_bytes = channel_id.as_bytes();
    if channel_id_bytes.len() > 32 {
        panic!("The passed &str should be <= 32 bytes");
    }

    let mut id: [u8; 32] = [b'0'; 32];
    for (index, byte) in id[32 - channel_id.len()..].iter_mut().enumerate() {
        *byte = channel_id_bytes[index];
    }

    ChannelId { id }
}

pub fn get_channel(id: &str, valid_until: &Option<DateTime<Utc>>, spec: Option<ChannelSpec>) -> Channel {
    let channel_id = get_channel_id(id);
    let deposit_amount = BigNum::try_from(<Faker as Number>::between(100_u32, 5000_u32)).expect("BigNum error when creating from random number");
    let valid_until: DateTime<Utc> = valid_until.unwrap_or(test_util::time::datetime_between(&Utc::now(), None));
    let creator = <Faker as Name>::name();
    let deposit_asset = get_asset();
    let spec = spec.unwrap_or(get_channel_spec(id, ValidatorsOption::Count(3)));

    Channel {
        id: channel_id,
        creator,
        deposit_asset,
        deposit_amount,
        valid_until,
        spec,
    }
}

pub fn get_channels(count: usize, valid_until_ge: Option<DateTime<Utc>>) -> Vec<Channel> {
    (1..=count)
        .map(|c| {
            // if we have a valid_until_ge, use it to generate a valid_util for each channel
            let valid_until = valid_until_ge.and_then(|ref dt| Some(test_util::time::datetime_between(dt, None)));
            let channel_id = format!("channel {}", c);

            get_channel(&channel_id, &valid_until, None)
        })
        .collect()
}

#[derive(Clone)]
#[allow(dead_code)]
pub enum ValidatorsOption {
    Count(usize),
    Some(Vec<ValidatorDesc>),
    None,
}

pub fn get_channel_spec(prefix: &str, validators_option: ValidatorsOption) -> ChannelSpec {
    let validators = match validators_option {
        ValidatorsOption::Count(count) => get_validators(count, Some(prefix)),
        ValidatorsOption::Some(validators) => validators,
        ValidatorsOption::None => vec![],
    };
    use crate::domain::EventSubmission;

    ChannelSpec {
        validators,
        title: Some("Title".to_string()),
        max_per_impression: BigNum::try_from(1).unwrap(),
        min_per_impression: BigNum::try_from(1).unwrap(),
        targeting: Vec::new(),
        min_targeting_score: Some(0),
        event_submission: EventSubmission {},
        created: Utc::now(),
        active_from: Some(Utc::now()),
        nonce: BigNum::try_from(0).unwrap(),
        withdraw_period_start: Utc::now(),
        ad_units: Vec::new(),
    }
}