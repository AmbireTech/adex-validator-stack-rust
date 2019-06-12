use serde::Serialize;
use tower_web::Response;

use domain::Channel;

#[derive(Debug, Response)]
pub struct ChannelListResponse {
    pub channels: Vec<Channel>,
    pub total_pages: usize,
}
