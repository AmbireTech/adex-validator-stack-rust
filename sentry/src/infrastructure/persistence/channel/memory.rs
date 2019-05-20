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

#[cfg(test)]
mod test {
    use crate::domain::{Channel, RepositoryError};
    use crate::domain::channel::ChannelRepository;
    use crate::domain::channel::fixtures::get_channel;
    use crate::infrastructure::persistence::channel::MemoryChannelRepository;

    #[test]
    fn initializes_with_channels_and_lists_channels() {
        futures::executor::block_on(async {
            let empty_init = MemoryChannelRepository::new(None);
            assert_eq!(0, await!(empty_init.list()).unwrap().len());

            let channels = [get_channel("channel 1"), get_channel("channel 2")];
            // this shouldn't change the order in any way
            let some_init = MemoryChannelRepository::new(Some(&channels));

            let channels_list: Vec<Channel> = await!(some_init.list()).expect("List the initial 2 channels");
            assert_eq!(2, channels_list.len());

            let last_channel = channels_list.last().expect("There should be a last Channel (total: 2)");
            assert_eq!("channel 2", last_channel.id);
        })
    }

    #[test]
    fn saves_channels() {
        futures::executor::block_on(async {
            let channels = [get_channel("XYZ")];

            let some_init = MemoryChannelRepository::new(Some(&channels));

            // get a 2nd channel to save
            let new_channel = get_channel("ABC");

            // save the 2nd channel
            // this shouldn't change the order in any way
            await!(some_init.save(new_channel)).expect("Saving 2nd new channel");

            let channels_list: Vec<Channel> = await!(some_init.list()).expect("List the 2 total channels");
            assert_eq!(2, channels_list.len());

            let last_channel = channels_list.last().expect("There should be a last Channel (total: 2)");
            assert_eq!("ABC", last_channel.id);

            // get a 3rd channel to save
            let new_channel = get_channel("DEF");

            // save the 2nd channel
            // this shouldn't change the order in any way
            await!(some_init.save(new_channel)).expect("Saving 3rd new channel");

            let channels_list: Vec<Channel> = await!(some_init.list()).expect("List the 3 total channels");
            assert_eq!(3, channels_list.len());

            let last_channel = channels_list.last().expect("There should be a last Channel (total: 3)");
            assert_eq!("DEF", last_channel.id);
        })
    }

    #[test]
    fn saving_the_same_channel_id_should_error() {
        futures::executor::block_on(async {
            let channels = [get_channel("ABC")];

            let repository = MemoryChannelRepository::new(Some(&channels));

            let same_channel_id = get_channel("ABC");

            let error = await!(repository.save(same_channel_id)).expect_err("It shouldn't be possible to save the same channel_id");
            match error {
                RepositoryError::UserError => {},
                _ => panic!("Expected UserError"),
            }
        })
    }
}