use futures::future::{FutureExt, TryFutureExt};
use futures_legacy::Future;
use tokio::await;
use tower_web::{derive_resource_impl, Deserialize, Extract, impl_web};

use channel_create::{ChannelCreateHandler, ChannelCreateResponse, ChannelInput};
use channel_list::{ChannelListHandler, ChannelListResponse};

use crate::infrastructure::persistence::channel::{MemoryChannelRepository, PostgresChannelRepository};
use crate::infrastructure::persistence::DbPool;

mod channel_list;
mod channel_create;

#[derive(Clone, Debug)]
pub struct ChannelResource {
    pub db_pool: DbPool,
    pub channel_list_limit: u32,
}

impl_web! {
    impl ChannelResource {
        #[post("/channel")]
        #[content_type("application/json")]
        async fn create_channel(&self, body: ChannelInput) -> ChannelCreateResponse {
            let _channel_repository = PostgresChannelRepository::new(self.db_pool.clone());
            let channel_repository = MemoryChannelRepository::new(None);

            let handler = ChannelCreateHandler::new(&channel_repository);

            await!(handler.handle(body).boxed().compat()).unwrap()
        }

        #[get("/channel/list")]
        #[content_type("application/json")]
        async fn channel_list(&self, query_string: ChannelListQuery) -> ChannelListResponse {
            let _channel_repository = PostgresChannelRepository::new(self.db_pool.clone());
            let channel_repository = MemoryChannelRepository::new(None);

            let handler = ChannelListHandler::new(self.channel_list_limit, &channel_repository);

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
        self
            .validator
            .to_owned()
            .and_then(|s| {
                if s.is_empty() {
                    return None;
                }

                Some(s)
            })
    }
}