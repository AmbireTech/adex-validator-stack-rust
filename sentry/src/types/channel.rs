use chrono::serde::ts_seconds;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tower_web::Extract;
use tower_web::Response;

use domain::{Asset, BigNum, ChannelId, ChannelSpec};

#[derive(Extract, Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ChannelInput {
    pub id: ChannelId,
    pub creator: String,
    pub deposit_asset: Asset,
    pub deposit_amount: BigNum,
    #[serde(with = "ts_seconds")]
    pub valid_until: DateTime<Utc>,
    pub spec: ChannelSpec,
}

#[derive(Debug, Response)]
pub struct ChannelCreateResponse {
    pub success: bool,
}

pub struct ChannelListHandler {
    limit_per_page: u32,
    channel_repository: Arc<dyn ChannelRepository>,
}