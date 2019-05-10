use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};

//use crate::domain::BigNum;
use crate::domain::ValidatorDesc;

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Channel {
    pub id: String,
    pub creator: String,
    pub deposit_asset: String,
    pub deposit_amount: i64,
    // @TODO: use BigNum and implement toSql
    pub valid_until: DateTime<Utc>,
//    pub spec: ChannelSpec,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ChannelSpec {
    validators: Vec<ValidatorDesc>,
}