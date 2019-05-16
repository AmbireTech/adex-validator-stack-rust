use std::sync::{Arc, RwLock};

use futures::future::{FutureExt, ok, err};

use crate::domain::{Channel, ChannelRepository, RepositoryError};
use crate::domain::RepositoryFuture;
use crate::infrastructure::persistence::memory::MemoryPersistenceError;

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
        let res_fut = match self.records.read() {
            Ok(reader) => {
                let channels = reader.iter().map(|channel| channel.clone()).collect();

                ok(channels)
            },
            Err(error) => err(
                RepositoryError::PersistenceError(
                    Box::new(
                        MemoryPersistenceError::from(error)
                    )
                )
            )
        };

        res_fut.boxed()
    }

    fn create(&self, channel: Channel) -> RepositoryFuture<()> {
        let create_fut = match self.records.write() {
            Ok(mut writer) => {
                writer.push(channel);

                ok(())
            },
            Err(error) => err(
                RepositoryError::PersistenceError(
                    Box::new(
                        MemoryPersistenceError::from(error)
                    )
                )
            )
        };

        create_fut.boxed()
    }
}