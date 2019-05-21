use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::domain::{Asset, RepositoryFuture, ValidatorDesc};
use crate::domain::bignum::BigNum;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Channel {
    pub id: String,
    pub creator: String,
    pub deposit_asset: Asset,
    pub deposit_amount: BigNum,
    pub valid_until: DateTime<Utc>,
//    pub spec: ChannelSpec,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ChannelSpec {
    validators: Vec<ValidatorDesc>,
}

pub trait ChannelRepository: Send + Sync {
    fn list(&self, valid_until_ge: DateTime<Utc>, page: u32, limit: u32) -> RepositoryFuture<Vec<Channel>>;

    fn save(&self, channel: Channel) -> RepositoryFuture<()>;

    fn find(&self, channel_id: &String) -> RepositoryFuture<Option<Channel>>;
}

#[cfg(test)]
pub(crate) mod fixtures {
    use std::convert::TryFrom;

    use chrono::{DateTime, Utc};
    use fake::faker::*;

    use crate::domain::{BigNum, Channel};
    use crate::domain::asset::fixtures::get_asset;
    use crate::test_util;

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
}