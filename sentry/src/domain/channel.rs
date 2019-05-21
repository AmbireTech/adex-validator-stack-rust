use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::domain::{Asset, DomainError, RepositoryFuture, ValidatorDesc};
use crate::domain::bignum::BigNum;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Channel {
    pub id: String,
    pub creator: String,
    pub deposit_asset: Asset,
    pub deposit_amount: BigNum,
    pub valid_until: DateTime<Utc>,
    pub spec: ChannelSpec,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ChannelSpec {
    // TODO: Use a ChannelSpecId?
    pub id: Uuid,
    pub validators: Vec<ValidatorDesc>,
}

pub struct ChannelListParams {
    /// page to show, should be >= 1
    pub page: u32,
    /// channels limit per page, should be >= 1
    pub limit: u32,
    /// filters `valid_until` to be `>= valid_until_ge`
    pub valid_until_ge: DateTime<Utc>,
    /// filters the channels containing a specific validator if provided
    // @TODO: use a ValidatorName struct, to have a better control of having a valid ValidatorName at this point
    pub validator: Option<String>,
    /// Ensures that this struct can only be created by calling `new()`
    _secret: (),
}

impl ChannelListParams {
    pub fn new(valid_until_ge: DateTime<Utc>, limit: u32, page: u32, validator: Option<String>) -> Result<Self, DomainError> {
        if page < 1 {
            return Err(DomainError::InvalidArgument("Page should be >= 1".to_string()));
        }

        if limit < 1 {
            return Err(DomainError::InvalidArgument("Limit should be >= 1".to_string()));
        }

        let validator = validator
            .and_then(|s| {
                if s.is_empty() {
                    return None;
                }

                Some(s)
            });

        Ok(Self {
            valid_until_ge,
            page,
            limit,
            validator,
            _secret: (),
        })
    }
}

pub trait ChannelRepository: Send + Sync {
    /// Returns a list of channels, based on the passed Parameters for this method
    fn list(&self, params: &ChannelListParams) -> RepositoryFuture<Vec<Channel>>;

    fn save(&self, channel: Channel) -> RepositoryFuture<()>;

    fn find(&self, channel_id: &String) -> RepositoryFuture<Option<Channel>>;
}

#[cfg(test)]
pub(crate) mod fixtures {
    use std::convert::TryFrom;

    use chrono::{DateTime, Utc};
    use fake::faker::*;
    use uuid::Uuid;

    use crate::domain::asset::fixtures::get_asset;
    use crate::domain::BigNum;
    use crate::domain::validator::fixtures::get_validators;
    use crate::test_util;

    use super::{Channel, ChannelSpec};

    pub fn get_channel(channel_id: &str, valid_until: &Option<DateTime<Utc>>) -> Channel {
        let deposit_amount = BigNum::try_from(<Faker as Number>::between(100_u32, 5000_u32)).expect("BigNum error when creating from random number");
        let valid_until: DateTime<Utc> = valid_until.unwrap_or(test_util::time::datetime_between(&Utc::now(), None));
        let creator = <Faker as Name>::name();
        let deposit_asset = get_asset();

        Channel {
            id: channel_id.to_string(),
            creator,
            deposit_asset,
            deposit_amount,
            valid_until,
            spec: get_channel_spec(Uuid::new_v4(), 3),
        }
    }

    pub fn get_channels(count: usize, valid_until_ge: Option<DateTime<Utc>>) -> Vec<Channel> {
        (1..=count)
            .map(|c| {
                // if we have a valid_until_ge, use it to generate a valid_util for each channel
                let valid_until = valid_until_ge.and_then(|ref dt| Some(test_util::time::datetime_between(dt, None)));
                let channel_id = format!("channel {}", c);

                get_channel(&channel_id, &valid_until)
            })
            .collect()
    }

    pub fn get_channel_spec(id: Uuid, validators_count: usize) -> ChannelSpec {
        ChannelSpec {
            id,
            validators: get_validators(validators_count, Some(&id.to_string())),
        }
    }
}