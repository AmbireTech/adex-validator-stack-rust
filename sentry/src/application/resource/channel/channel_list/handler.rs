use tokio::await;

use crate::domain::ChannelRepository;

use super::ChannelListResponse;

pub struct ChannelListHandler<'a> {
    channel_repository: &'a dyn ChannelRepository,
}

impl<'a> ChannelListHandler<'a> {
    pub fn new(channel_repository: &'a ChannelRepository) -> Self {
        Self { channel_repository }
    }
}

impl<'a> ChannelListHandler<'a> {
    pub async fn handle(&self) -> Result<ChannelListResponse, ()> {
        // @TODO: Proper error handling
        let channels = await!(self.channel_repository.list()).unwrap();

        Ok(ChannelListResponse { channels })
    }
}