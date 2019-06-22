use futures::future::{ready, FutureExt};

use domain::{Channel, ChannelId, RepositoryFuture};
use memory_repository::MemoryRepository;

use crate::domain::channel::{ChannelListParams, ChannelRepository};
use std::sync::Arc;

#[cfg(test)]
#[path = "./memory_test.rs"]
mod memory_test;

pub struct MemoryChannelRepository {
    inner: MemoryRepository<Channel, ChannelId>,
}

impl MemoryChannelRepository {
    pub fn new(initial_channels: Option<&[Channel]>) -> Self {
        let initial_channels = initial_channels.unwrap_or(&[]).to_vec();
        let cmp: Arc<dyn Fn(&Channel, &ChannelId) -> bool + Send + Sync> =
            Arc::new(|left, right| &left.id == right);

        Self {
            inner: MemoryRepository::new(&initial_channels, cmp),
        }
    }
}

impl ChannelRepository for MemoryChannelRepository {
    fn list(&self, params: &ChannelListParams) -> RepositoryFuture<Vec<Channel>> {
        let result = self
            .inner
            .list(params.limit, params.page, |channel| {
                list_filter(&params, channel)
            })
            .map_err(|error| error.into());

        ready(result).boxed()
    }

    fn list_count(&self, params: &ChannelListParams) -> RepositoryFuture<u64> {
        let result = self
            .inner
            .list_all(|channel| list_filter(&params, channel))
            .map(|channels| {
                let filtered_count = channels.len();
                (filtered_count as f64 / f64::from(params.limit)).ceil() as u64
            })
            .map_err(|error| error.into());

        ready(result).boxed()
    }

    fn find(&self, channel_id: &ChannelId) -> RepositoryFuture<Option<Channel>> {
        let result = self.inner.find(channel_id).map_err(|error| error.into());

        ready(result).boxed()
    }

    fn add(&self, channel: Channel) -> RepositoryFuture<()> {
        let channel_id = channel.id.clone();
        let result = self
            .inner
            .add(&channel_id, channel)
            .map_err(|error| error.into());

        ready(result).boxed()
    }
}

fn list_filter(params: &ChannelListParams, channel: &Channel) -> Option<Channel> {
    let valid_until_filter = channel.valid_until >= params.valid_until_ge;

    let validator_filter_passed = match &params.validator {
        Some(validator_id) => {
            // check if there is any validator in the current
            // `channel.spec.validators` that has the same `id`
            channel.spec.validators.find(&validator_id).is_some()
        }
        // if None -> the current channel has passed, since we don't need to filter by anything
        None => true,
    };

    match (valid_until_filter, validator_filter_passed) {
        (true, true) => Some(channel.clone()),
        (_, _) => None,
    }
}
