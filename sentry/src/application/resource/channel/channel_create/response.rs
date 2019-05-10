use serde::Serialize;
use tower_web::Response;

use crate::domain::Channel;

#[derive(Debug, Response)]
pub struct ChannelCreateResponse {
    pub success: bool,
}