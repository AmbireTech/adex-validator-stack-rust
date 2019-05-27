use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tower_web::Extract;

use crate::domain::{Asset, BigNum, ChannelId};

#[derive(Extract, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ChannelInput {
    pub id: ChannelId,
    pub creator: String,
    pub deposit_asset: Asset,
    pub deposit_amount: BigNum,
    pub valid_until: DateTime<Utc>,
}