use std::sync::{Arc, RwLock};

use futures::future::{err, FutureExt, ok};

use crate::domain::{Channel, ChannelRepository, RepositoryError, RepositoryFuture};

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
        let channel_found = match self.records.read() {
            Ok(reader) => {
                reader.iter().find_map(|current| {
                    match &channel.id == &current.id {
                        true => Some(()),
                        false => None
                    }
                })
            }
            Err(error) => return err(error.into()).boxed(),
        };

        if channel_found.is_some() {
            return err(RepositoryError::UserError).boxed();
        }

        let create_fut = match self.records.write() {
            Ok(mut writer) => {
                writer.push(channel);

                ok(())
            }
            Err(error) => err(error.into())
        };

        create_fut.boxed()
    }

    fn find(&self, channel_id: &String) -> RepositoryFuture<Option<Channel>> {
        let res_fut = match self.records.read() {
            Ok(reader) => {
                let found_channel = reader.iter().find_map(|channel| {
                    match &channel.id == channel_id {
                        true => Some(channel.clone()),
                        false => None
                    }
                });

                ok(found_channel)
            }
            Err(error) => err(error.into()),
        };

        res_fut.boxed()
    }
}