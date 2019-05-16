use std::sync::{Arc, RwLock};

use crate::domain::RepositoryFuture;
use crate::domain::{Channel, ChannelRepository};

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
        Box::pin(
            futures::future::ok(
                // @TODO: instead of Unwrap, unwrap it in a RepositoryError
                self.records.read().unwrap().iter().map(|channel| channel.clone()).collect()
            )
        )
    }

    fn create(&self, channel: Channel) -> RepositoryFuture<()> {
        // @TODO: instead of Unwrap, unwrap it in a RepositoryError
        &self.records.write().unwrap().push(channel);

        Box::pin(
            futures::future::ok(
                ()
            )
        )
    }
}