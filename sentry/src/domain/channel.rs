use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::domain::{RepositoryFuture, ValidatorDesc};
use crate::domain::bignum::BigNum;

#[derive(Serialize, Deserialize, Debug)]
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

    fn create(&self, channel: Channel) -> RepositoryFuture<()>;
}