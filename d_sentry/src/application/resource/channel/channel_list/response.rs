use serde::Serialize;
use tower_web::Response;

use domain::Channel;

#[derive(Debug, Response, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelListResponse {
    pub channels: Vec<Channel>,
    pub total_pages: u64,
}
