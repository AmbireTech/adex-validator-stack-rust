use std::sync::{Arc, RwLock};

use futures::future::{err, ok, FutureExt};

use crate::domain::channel::{ChannelListParams, ChannelRepository};
use crate::infrastructure::persistence::memory::MemoryPersistenceError;
use domain::{Channel, ChannelId, RepositoryError, RepositoryFuture};
use memory_repository::MemoryRepository;

#[cfg(test)]
#[path = "./memory_test.rs"]
mod memory_test;

pub struct MemoryChannelRepository {
    records: Arc<RwLock<Vec<Channel>>>,
    inner: MemoryRepository<Channel>,
}

impl MemoryChannelRepository {
    pub fn new(initial_channels: Option<&[Channel]>) -> Self {
        let memory_channels = initial_channels.unwrap_or(&[]).to_vec();

        let cmp: Arc<dyn Fn(&Channel, &Channel) -> bool + Send + Sync> =
            Arc::new(|left, right| left.id == right.id);

        Self {
            records: Arc::new(RwLock::new(memory_channels)),
            inner: MemoryRepository::new(&[], cmp),
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

        futures::future::ready(result).boxed()
    }

    fn list_count(&self, params: &ChannelListParams) -> RepositoryFuture<u64> {
        let res_fut = match self.records.read() {
            Ok(reader) => {
                let filtered_count = reader
                    .iter()
                    .filter_map(|channel| list_filter(&params, channel))
                    .count();
                let pages = (filtered_count as f64 / f64::from(params.limit)).ceil() as u64;
                ok(pages)
            }
            Err(error) => err(MemoryPersistenceError::from(error).into()),
        };

        res_fut.boxed()
    }

    fn find(&self, channel_id: &ChannelId) -> RepositoryFuture<Option<Channel>> {
        let res_fut = match self.records.read() {
            Ok(reader) => {
                let found_channel = reader.iter().find_map(|channel| {
                    if &channel.id == channel_id {
                        Some(channel.clone())
                    } else {
                        None
                    }
                });

                ok(found_channel)
            }
            Err(error) => err(MemoryPersistenceError::from(error).into()),
        };

        res_fut.boxed()
    }

    fn add(&self, channel: Channel) -> RepositoryFuture<()> {
        let channel_found = match self.records.read() {
            Ok(reader) => reader.iter().find_map(|current| {
                if channel.id == current.id {
                    Some(())
                } else {
                    None
                }
            }),
            Err(error) => return err(MemoryPersistenceError::from(error).into()).boxed(),
        };

        if channel_found.is_some() {
            // @TODO: Implement an AlreadyExist Error
            return err(RepositoryError::User).boxed();
        }

        let create_fut = match self.records.write() {
            Ok(mut writer) => {
                writer.push(channel);

                ok(())
            }
            Err(error) => err(MemoryPersistenceError::from(error).into()),
        };

        create_fut.boxed()
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
