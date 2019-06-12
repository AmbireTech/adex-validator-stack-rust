use chrono::Utc;
use tokio::await;

use crate::domain::channel::{ChannelListParams, ChannelRepository};

use super::ChannelListResponse;
use std::sync::Arc;

pub struct ChannelListHandler {
    limit_per_page: u32,
    channel_repository: Arc<dyn ChannelRepository>,
}

impl ChannelListHandler {
    pub fn new(limit_per_page: u32, channel_repository: Arc<dyn ChannelRepository>) -> Self {
        Self {
            limit_per_page,
            channel_repository,
        }
    }
}

impl ChannelListHandler {
    #[allow(clippy::needless_lifetimes)]
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
        let channels_count =
            await!(self.channel_repository.list_count(&channel_list_params)).unwrap();

        Ok(ChannelListResponse {
            channels,
            total_pages: channels_count,
        })
    }
}
