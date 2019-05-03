use hyper::{Body, Method, Request, Response};
use tokio::await;

use crate::application::error::ApplicationError;
use crate::application::handler::channel::ChannelListHandler;
use crate::infrastructure::http::route::{RequestPath, RoutePath};
use crate::infrastructure::persistence::channel::PostgresChannelRepository;
use crate::infrastructure::persistence::DbPool;

pub enum SentryRequest {
    ChannelList,
//    ChannelCreate(Channel),
//    ChannelRequest,
}

impl SentryRequest {
    pub async fn handle(db_pool: DbPool, sentry_request: SentryRequest) -> Result<Response<Body>, ApplicationError> {
        let channel_repository = PostgresChannelRepository::new(db_pool.clone());

        match sentry_request {
            SentryRequest::ChannelList => {
                let channel_list_handler = ChannelListHandler::new(&channel_repository);

                return Ok(await!(channel_list_handler.handle()));
            }
        }
    }
}

pub async fn request_router(request: Request<Body>) -> Result<SentryRequest, ApplicationError> {
    let request_path = RequestPath::new(request.method().clone(), request.uri().path());

    if RoutePath::new(Method::GET, "/channel/list").is_match(&request_path) {
        return Ok(SentryRequest::ChannelList);
    }

    Err(ApplicationError::NotFound)
}
