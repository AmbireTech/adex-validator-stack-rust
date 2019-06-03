use serde::Serialize;
use tower_web::Response;

#[derive(Debug, Response)]
pub struct ChannelCreateResponse {
    pub success: bool,
}
