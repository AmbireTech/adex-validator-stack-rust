use serde_derive::*;
use crate::bignum::BigNum;
use crate::validator::ValidatorDesc;

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all="camelCase")]
pub struct Channel {
    id: String,
    creator: String,
    deposit_asset: String,
    deposit_amount: BigNum,
    valid_until: u64,
    spec: ChannelSpec,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all="camelCase")]
pub struct ChannelSpec {
    validators: Vec<ValidatorDesc>,
}