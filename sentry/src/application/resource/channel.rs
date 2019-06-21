use futures::future::{FutureExt, TryFutureExt};
use futures_legacy::Future;
use tokio::await;
use tower_web::{derive_resource_impl, impl_web, Deserialize, Extract};

use channel_create::{ChannelCreateHandler, ChannelCreateResponse, ChannelInput};
use channel_list::{ChannelListHandler, ChannelListResponse};

use crate::domain::channel::ChannelRepository;
use std::sync::Arc;

mod channel_create;
mod channel_list;

#[derive(Clone)]
pub struct ChannelResource {
    pub channel_list_limit: u32,
    pub channel_repository: Arc<dyn ChannelRepository>,
}

impl_web! {
    #[allow(clippy::needless_lifetimes)]
    impl ChannelResource {
        #[post("/channel")]
        #[content_type("application/json")]
        async fn create_channel(&self, body: ChannelInput) -> ChannelCreateResponse {

            let handler = ChannelCreateHandler::new(self.channel_repository.clone());

            await!(handler.handle(body).boxed().compat()).unwrap()
        }

        #[get("/channel/list")]
        #[content_type("application/json")]
        async fn channel_list(&self, query_string: ChannelListQuery) -> ChannelListResponse {
            let handler = ChannelListHandler::new(self.channel_list_limit, self.channel_repository.clone());

            await!(handler.handle(query_string.page(), query_string.validator()).boxed().compat()).unwrap()
        }
    }
}

#[derive(Extract)]
struct ChannelListQuery {
    page: Option<u32>,
    validator: Option<String>,
}

impl ChannelListQuery {
    pub fn page(&self) -> u32 {
        match self.page {
            Some(page) if page >= 1 => page,
            _ => 1,
        }
    }

    pub fn validator(&self) -> Option<String> {
        self.validator.to_owned().and_then(|s| {
            if s.is_empty() {
                return None;
            }

            Some(s)
        })
    }
}
