use std::convert::TryFrom;

use chrono::{DateTime, Utc};
use fake::faker::*;
use uuid::Uuid;

use crate::domain::asset::fixtures::get_asset;
use crate::domain::BigNum;
use crate::domain::validator::fixtures::get_validators;
use crate::domain::ValidatorDesc;
use crate::test_util;

use super::{Channel, ChannelSpec};

pub fn get_channel(channel_id: &str, valid_until: &Option<DateTime<Utc>>, spec: Option<ChannelSpec>) -> Channel {
    let deposit_amount = BigNum::try_from(<Faker as Number>::between(100_u32, 5000_u32)).expect("BigNum error when creating from random number");
    let valid_until: DateTime<Utc> = valid_until.unwrap_or(test_util::time::datetime_between(&Utc::now(), None));
    let creator = <Faker as Name>::name();
    let deposit_asset = get_asset();
    let spec = spec.unwrap_or(get_channel_spec(Uuid::new_v4(), ValidatorsOption::Count(3)));

    Channel {
        id: channel_id.to_string(),
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

pub fn get_channel_spec(id: Uuid, validators_option: ValidatorsOption) -> ChannelSpec {
    let validators = match validators_option {
        ValidatorsOption::Count(count) => get_validators(count, Some(&id.to_string())),
        ValidatorsOption::Some(validators) => validators,
        ValidatorsOption::None => vec![],
    };

    ChannelSpec { id, validators }
}