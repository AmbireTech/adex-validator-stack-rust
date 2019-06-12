use tokio::await;

use crate::domain::channel::ChannelRepository;
use domain::Channel;

use super::ChannelCreateResponse;
use super::ChannelInput;
use std::sync::Arc;

pub struct ChannelCreateHandler {
    channel_repository: Arc<dyn ChannelRepository>,
}

impl ChannelCreateHandler {
    pub fn new(channel_repository: Arc<dyn ChannelRepository>) -> Self {
        Self { channel_repository }
    }
}

impl ChannelCreateHandler {
    #[allow(clippy::needless_lifetimes)]
    pub async fn handle(&self, channel_input: ChannelInput) -> Result<ChannelCreateResponse, ()> {
        // @TODO: Creating Channel Validation

        let channel = Channel {
            id: channel_input.id,
            creator: channel_input.creator,
            deposit_asset: channel_input.deposit_asset,
            deposit_amount: channel_input.deposit_amount,
            valid_until: channel_input.valid_until,
            spec: channel_input.spec,
        };

        let success = await!(self.channel_repository.create(channel)).is_ok();

        Ok(ChannelCreateResponse { success })
    }
}
