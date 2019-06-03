use chrono::Utc;
use tokio::await;

use domain::{ChannelListParams, ChannelRepository};

use super::ChannelListResponse;

pub struct ChannelListHandler<'a> {
    limit_per_page: u32,
    channel_repository: &'a dyn ChannelRepository,
}

impl<'a> ChannelListHandler<'a> {
    pub fn new(limit_per_page: u32, channel_repository: &'a ChannelRepository) -> Self {
        Self {
            limit_per_page,
            channel_repository,
        }
    }
}

impl<'a> ChannelListHandler<'a> {
    pub async fn handle(
        &self,
        page: u32,
        validator: Option<String>,
    ) -> Result<ChannelListResponse, ()> {
        let channel_list_params =
            ChannelListParams::new(Utc::now(), self.limit_per_page, page, validator)
                .expect("Params should be generated from valid data.");

        let list_fut = self.channel_repository.list(&channel_list_params);
        // @TODO: Proper error handling
        let channels = await!(list_fut).unwrap();

        Ok(ChannelListResponse { channels })
    }
}
