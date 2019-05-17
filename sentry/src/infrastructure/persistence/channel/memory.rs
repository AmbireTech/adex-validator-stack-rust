use std::sync::{Arc, RwLock};

use futures::future::{err, FutureExt, ok};

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
        let res_fut = match self.records.read() {
            Ok(reader) => {
                let channels = reader.iter().map(|channel| channel.clone()).collect();

                ok(channels)
            }
            Err(error) => err(error.into())
        };

        res_fut.boxed()
    }

    fn save(&self, channel: Channel) -> RepositoryFuture<()> {

        let create_fut = match self.records.write() {
            Ok(mut writer) => {
                writer.push(channel);

                ok(())
            }
            Err(error) => err(error.into())
        };

        create_fut.boxed()
    }

    fn find(&self, _channel_id: String) -> RepositoryFuture<Option<Channel>> {
        ok(None).boxed()
    }
}