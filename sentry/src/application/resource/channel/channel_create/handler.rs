use tokio::await;

use crate::domain::channel::ChannelRepository;
use domain::Channel;

use super::ChannelCreateResponse;
use super::ChannelInput;

pub struct ChannelCreateHandler<'a> {
    channel_repository: &'a dyn ChannelRepository,
}

impl<'a> ChannelCreateHandler<'a> {
    pub fn new(channel_repository: &'a dyn ChannelRepository) -> Self {
        Self { channel_repository }
    }
}

impl<'a> ChannelCreateHandler<'a> {
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
