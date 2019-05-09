use serde::Serialize;
use tower_web::Response;

use crate::domain::Channel;

#[derive(Debug, Response)]
pub struct ChannelListResponse {
    pub channels: Vec<Channel>,
}