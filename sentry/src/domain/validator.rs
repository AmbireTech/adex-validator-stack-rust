use crate::domain::BigNum;
use serde_derive::*;

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all="camelCase")]
pub struct ValidatorDesc {
    id: String,
    url: String,
    fee: BigNum,
}