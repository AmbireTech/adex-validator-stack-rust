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
    fn list(&self) -> RepositoryFuture<Vec<Channel>>;

    fn save(&self, channel: Channel) -> RepositoryFuture<()>;

    fn find(&self, channel_id: &String) -> RepositoryFuture<Option<Channel>>;
}

#[cfg(test)]
pub(crate) mod fixtures {
    use std::convert::TryFrom;

    use chrono::{DateTime, Utc};
    use fake::faker::*;
    use fake::helper::take_one;

    use crate::domain::{BigNum, Channel};
    use crate::test_util;

    pub fn get_channel(channel_id: &str) -> Channel {
        let deposit_assets = ["DAI", "BGN", "EUR", "USD", "ADX", "BTC", "LIT", "ETH"];
        let deposit_amount = BigNum::try_from(<Faker as Number>::between(100_u32, 5000_u32)).expect("BigNum error when creating from random number");

        let valid_until: DateTime<Utc> = test_util::time::datetime_between(&Utc::now(), None);

        Channel {
            id: channel_id.to_string(),
            creator: <Faker as Name>::name(),
            deposit_asset: take_one(&deposit_assets).into(),
            deposit_amount,
            valid_until,
        }
    }
}