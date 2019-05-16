use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::domain::bignum::BigNum;
use crate::domain::ValidatorDesc;

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