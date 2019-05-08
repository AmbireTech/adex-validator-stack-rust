use serde::{Deserialize, Serialize};
use tower_web::Extract;
use chrono::{DateTime, Utc};

#[derive(Extract, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ChannelInput {
    pub id: String,
    pub creator: String,
    pub deposit_asset: String,
    pub deposit_amount: i32,
    pub valid_until: DateTime<Utc>,
}