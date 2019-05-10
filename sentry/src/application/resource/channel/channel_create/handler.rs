use tokio::await;

use crate::infrastructure::persistence::channel::PostgresChannelRepository;

use super::ChannelCreateResponse;
use super::ChannelInput;

pub struct ChannelCreateHandler<'a> {
    channel_repository: &'a PostgresChannelRepository,
}

impl<'a> ChannelCreateHandler<'a> {
    pub fn new(channel_repository: &'a PostgresChannelRepository) -> Self {
        Self { channel_repository }
    }
}

impl<'a> ChannelCreateHandler<'a> {
    pub async fn handle(&self, _channel_input: ChannelInput) -> Result<ChannelCreateResponse, ()> {
        // @TODO: Creating Channel Validation

        // @TODO: Insert into database

        Ok(ChannelCreateResponse { success: true })
    }
}