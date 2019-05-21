use tokio::await;

use crate::domain::ChannelRepository;

use super::ChannelListResponse;
use chrono::Utc;

pub struct ChannelListHandler<'a> {
    limit_per_page: u32,
    channel_repository: &'a dyn ChannelRepository,
}

impl<'a> ChannelListHandler<'a> {
    pub fn new(limit_per_page: u32, channel_repository: &'a ChannelRepository) -> Self {
        Self { limit_per_page, channel_repository }
    }
}

impl<'a> ChannelListHandler<'a> {
    pub async fn handle(&self, page: u32, _validator: Option<String>) -> Result<ChannelListResponse, ()> {
        // @TODO: pass validator to `list()` once we implement the search in Repository.
        let list_fut = self.channel_repository.list(Utc::now(), page, self.limit_per_page);
        // @TODO: Proper error handling
        let channels = await!(list_fut).unwrap();

        Ok(ChannelListResponse { channels })
    }
}