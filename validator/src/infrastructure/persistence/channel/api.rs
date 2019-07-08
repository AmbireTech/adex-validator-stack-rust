use futures::{FutureExt, TryFutureExt};

use domain::{Channel, RepositoryFuture};

use crate::domain::channel::ChannelRepository;
use crate::infrastructure::persistence::api::ApiPersistenceError;
use crate::infrastructure::sentry::SentryApi;

// @TODO: make pub(crate)
pub struct ApiChannelRepository {
    pub sentry: SentryApi,
}

impl ChannelRepository for ApiChannelRepository {
    fn all(&self, identity: &str) -> RepositoryFuture<Vec<Channel>> {
        self.sentry
            .clone()
            .all_channels(Some(identity.to_string()))
            // @TODO: Error handling
            .map_err(|_error| ApiPersistenceError::Reading.into())
            .boxed()
    }
}
