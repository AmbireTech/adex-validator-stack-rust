use std::sync::Arc;

use futures::future::{ready, FutureExt};

use domain::{Channel, ChannelId, RepositoryFuture, SpecValidator};
use memory_repository::MemoryRepository;

use crate::domain::channel::ChannelRepository;

// @TODO: make pub(crate)
pub struct MemoryChannelRepository {
    inner: MemoryRepository<Channel, ChannelId>,
}

impl MemoryChannelRepository {
    pub fn new(initial_messages: &[Channel]) -> Self {
        let cmp = Arc::new(|channel: &Channel, channel_id: &ChannelId| &channel.id == channel_id);

        Self {
            inner: MemoryRepository::new(&initial_messages, cmp),
        }
    }
}

impl ChannelRepository for MemoryChannelRepository {
    fn all(&self, identity: &str) -> RepositoryFuture<Vec<Channel>> {
        let list = self
            .inner
            .list_all(|channel| match channel.spec.validators.find(identity) {
                SpecValidator::Leader(_) | SpecValidator::Follower(_) => Some(channel.clone()),
                SpecValidator::None => None,
            });

        ready(list.map_err(Into::into)).boxed()
    }
}
