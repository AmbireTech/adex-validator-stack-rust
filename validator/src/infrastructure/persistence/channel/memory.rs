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
    pub fn new(initial_channels: &[Channel]) -> Self {
        let cmp = Arc::new(|channel: &Channel, channel_id: &ChannelId| &channel.id == channel_id);

        Self {
            inner: MemoryRepository::new(&initial_channels, cmp),
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

#[cfg(test)]
mod test {
    use domain::fixtures::{get_channel, get_channel_id, get_channel_spec, get_validator};
    use domain::SpecValidators;

    use super::*;
    use domain::channel::fixtures::ValidatorsOption;

    #[test]
    fn find_all_channels_with_the_passed_identity_and_skips_the_rest() {
        futures::executor::block_on(async {
            let identity = "Lookup identity";
            let validator_1 = get_validator(identity, None);
            let validator_2 = get_validator("Second", None);
            let channel_1_spec = get_channel_spec(ValidatorsOption::Pair {
                leader: validator_1.clone(),
                follower: validator_2.clone(),
            });

            // switch leader & follower
            let channel_3_spec = get_channel_spec(ValidatorsOption::Pair {
                leader: validator_2.clone(),
                follower: validator_1.clone(),
            });
            let channels = vec![
                get_channel("channel 1", &None, Some(channel_1_spec)),
                get_channel("channel 2", &None, None),
                get_channel("channel 3", &None, Some(channel_3_spec)),
            ];

            let repository = MemoryChannelRepository::new(&channels);

            let result: Vec<Channel> =
                await!(repository.all(&identity)).expect("Getting all channels failed");

            assert_eq!(2, result.len());
            assert_eq!(&get_channel_id("channel 1"), &result[0].id);
            assert_eq!(&get_channel_id("channel 3"), &result[1].id);
        })
    }
}
