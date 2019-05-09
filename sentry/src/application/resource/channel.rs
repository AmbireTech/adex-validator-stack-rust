use futures::future::{FutureExt, TryFutureExt};

use futures_legacy::Future;
use tokio::await;
use tower_web::{derive_resource_impl, impl_web};

use channel_list::ChannelListHandler;
use channel_list::ChannelListResponse;

use crate::infrastructure::persistence::channel::PostgresChannelRepository;
use crate::infrastructure::persistence::DbPool;

mod channel_list;

#[derive(Clone, Debug)]
pub struct ChannelResource {
    pub db_pool: DbPool,
}

impl_web! {
    impl ChannelResource {
        #[get("/channel/list")]
        #[content_type("application/json")]
        async fn channel_list(&self) -> ChannelListResponse {
            let channel_repository = PostgresChannelRepository::new(self.db_pool.clone());

            let handler = ChannelListHandler::new(&channel_repository);

            await!(handler.handle().boxed().compat()).unwrap()
        }
    }
}