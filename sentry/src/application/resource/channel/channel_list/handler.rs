use tokio::await;

use crate::domain::ChannelRepository;

use super::ChannelListResponse;
use chrono::Utc;

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
        // @TODO: pass the correct page from Query and use the application limit per page
        let list_fut = self.channel_repository.list(Utc::now(), 1, 200);
        // @TODO: Proper error handling
        let channels = await!(list_fut).unwrap();

        Ok(ChannelListResponse { channels })
    }
}