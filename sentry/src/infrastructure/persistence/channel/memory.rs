use std::sync::{Arc, RwLock};

use futures::future::{FutureExt, ok};

use crate::domain::{Channel, ChannelRepository};
use crate::domain::RepositoryFuture;

pub struct MemoryChannelRepository {
    records: Arc<RwLock<Vec<Channel>>>,
}

impl MemoryChannelRepository {
    pub fn new(initial_channels: Option<&[Channel]>) -> Self {
        let memory_channels = initial_channels.unwrap_or(&[]).to_vec();

        Self { records: Arc::new(RwLock::new(memory_channels)) }
    }
}

impl ChannelRepository for MemoryChannelRepository {
    fn list(&self) -> RepositoryFuture<Vec<Channel>> {
        // @TODO: instead of Unwrap, unwrap it in a RepositoryError
        let list = self.records.read().unwrap().iter().map(|channel| channel.clone()).collect();

        ok(list).boxed()
    }

    fn create(&self, channel: Channel) -> RepositoryFuture<()> {
        // @TODO: instead of Unwrap, unwrap it in a RepositoryError
        &self.records.write().unwrap().push(channel);

        ok(()).boxed()
    }
}