use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::domain::{RepositoryFuture, ValidatorDesc};
use crate::domain::bignum::BigNum;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Channel {
    pub id: String,
    pub creator: String,
    pub deposit_asset: String,
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
    use chrono::{DateTime, Utc};
    use fake::faker::*;
    use fake::helper::take_one;
    use num_bigint::BigUint;

    use crate::domain::{BigNum, Channel};
    use time::Duration;

    pub fn get_channel(channel_id: &str) -> Channel {
        let deposit_assets = ["DAI", "BGN", "EUR", "USD", "ADX", "BTC", "LIT", "ETH"];
        let rand_deposit: u32 = <Faker as Number>::between(100, 5000);
        let deposit_amount = BigNum::new(BigUint::from(rand_deposit)).expect("BigNum error when creating from random number");

        let valid_until_between = (
            Utc::now(),
            Utc::now() + Duration::days(365),
        );

        let valid_until: DateTime<Utc> = <Faker as Chrono>::between(
            None,
            &valid_until_between.0.to_rfc3339(),
            &valid_until_between.1.to_rfc3339(),
        ).parse().expect("Whoops, DateTime<Utc> should be created from Fake...");

        Channel {
            id: channel_id.to_string(),
            creator: <Faker as Name>::name(),
            deposit_asset: take_one(&deposit_assets).to_string(),
            deposit_amount,
            valid_until,
        }
    }
}