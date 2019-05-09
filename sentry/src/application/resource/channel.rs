use futures::future::{FutureExt, TryFutureExt};

use futures_legacy::Future;
use tokio::await;
use tower_web::{derive_resource_impl, impl_web};

use channel_list::ChannelListHandler;
use channel_list::ChannelListResponse;
use channel_create::ChannelInput;

use crate::infrastructure::persistence::channel::PostgresChannelRepository;
use crate::infrastructure::persistence::DbPool;

mod channel_list;
mod channel_create;

#[derive(Clone, Debug)]
pub struct ChannelResource {
    pub db_pool: DbPool,
}

impl_web! {
    impl ChannelResource {
        #[post("/channel")]
        #[content_type("application/json")]
        async fn create_channel(&self, body: ChannelInput) -> String {
            // @TODO: Create Channel in Database
            serde_json::to_string(&body).unwrap()
        }

        #[get("/channel/list")]
        #[content_type("application/json")]
        async fn channel_list(&self) -> ChannelListResponse {
            let channel_repository = PostgresChannelRepository::new(self.db_pool.clone());

            let handler = ChannelListHandler::new(&channel_repository);

            await!(handler.handle().boxed().compat()).unwrap()
        }
    }
}