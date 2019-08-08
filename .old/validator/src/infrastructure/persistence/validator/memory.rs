use std::sync::Arc;

use futures::future::{ready, FutureExt};

use domain::validator::message::{MessageType, State};
use domain::validator::{Message, ValidatorId};
use domain::{ChannelId, RepositoryFuture, ValidatorDesc};
use memory_repository::MemoryRepository;

use crate::domain::validator::repository::MessageRepository;

#[derive(Clone)]
pub struct MemoryState {}

impl State for MemoryState {
    type Signature = String;
    type StateRoot = String;
}

#[derive(Clone)]
pub struct MemoryMessage {
    pub message: Message<MemoryState>,
    pub channel: ChannelId,
    /// As we are not dealing with any URL requests here, we can only store and use the ValidatorId
    pub owner: ValidatorId,
}

pub struct MemoryMessageRepository {
    inner: MemoryRepository<MemoryMessage, bool>,
}

impl MemoryMessageRepository {
    pub fn new(initial_messages: &[MemoryMessage]) -> Self {
        let cmp = Arc::new(|_message: &MemoryMessage, should_match: &bool| *should_match);

        Self {
            inner: MemoryRepository::new(&initial_messages, cmp),
        }
    }
}

impl MessageRepository<MemoryState> for MemoryMessageRepository {
    fn add(
        &self,
        for_channel: &ChannelId,
        to_validator: &ValidatorDesc,
        message: Message<MemoryState>,
    ) -> RepositoryFuture<()> {
        let message = MemoryMessage {
            message,
            channel: *for_channel,
            owner: to_validator.id.clone(),
        };
        // this should never match against the new record, that's why always pass false.
        ready(self.inner.add(&false, message).map_err(Into::into)).boxed()
    }

    /// Fetches the latest Message of Channel from the given Validator,
    /// filtering by Types if provided.
    /// If not `types` are provided, it will match against all types.
    fn latest(
        &self,
        channel: &ChannelId,
        from: &ValidatorId,
        types: Option<&[&MessageType]>,
    ) -> RepositoryFuture<Option<Message<MemoryState>>> {
        let latest = self
            .inner
            .list_all(|mem_msg| {
                let is_from = &mem_msg.owner == from;
                let is_channel = &mem_msg.channel == channel;
                // if there are no types provided, it should match every type, i.e. default `true` for `None`
                let is_in_types = types.map_or(true, |message_types| {
                    mem_msg.message.is_types(message_types)
                });

                match (is_from, is_channel, is_in_types) {
                    (true, true, true) => Some(mem_msg.clone()),
                    (_, _, _) => None,
                }
            })
            .map(|mut memory_messages| memory_messages.pop().map(|mem| mem.message));

        ready(latest.map_err(Into::into)).boxed()
    }
}

#[cfg(test)]
mod test {
    use std::convert::TryFrom;

    use domain::fixtures::get_channel_id;
    use domain::validator::message::fixtures::{get_heartbeat, get_reject_state};

    use super::*;
    use domain::validator::fixtures::get_validator;
    use domain::validator::message::TYPE_REJECT;

    fn get_reject_memory_message(
        channel: &ChannelId,
        owner: &ValidatorId,
        reject_reason: Option<String>,
    ) -> MemoryMessage {
        MemoryMessage {
            channel: channel.clone(),
            owner: owner.clone(),
            message: Message::RejectState(get_reject_state(reject_reason)),
        }
    }

    #[test]
    fn adds_message_for_validator() {
        futures::executor::block_on(async {
            let validator = get_validator("identity", None);
            let repo = MemoryMessageRepository::new(&[]);

            let message = get_reject_state(None);
            let channel_id = get_channel_id("channel id");

            await!(repo.add(&channel_id, &validator, Message::RejectState(message)))
                .expect("Adding a message failed");

            let list_all = repo
                .inner
                .list_all(|m| Some(m.clone()))
                .expect("Listing all Messages failed");

            assert_eq!(1, list_all.len());
            assert_eq!(validator.id, list_all[0].owner);
            assert_eq!(channel_id, list_all[0].channel);
        })
    }

    #[test]
    fn getting_latest_message() {
        futures::executor::block_on(async {
            let validator = get_validator("identity", None);
            let channel = get_channel_id("channel id");

            let repo = MemoryMessageRepository::new(&[]);
            // add an initial Reject message for checking latest ordering
            let init_message =
                Message::RejectState(get_reject_state(Some("Initial Message".to_string())));
            await!(repo.add(&channel, &validator, init_message))
                .expect("Adding the initial message failed");

            let new_message = Message::RejectState(get_reject_state(Some("my reason".to_string())));
            await!(repo.add(&channel, &validator, new_message)).expect("Adding a message failed");

            let latest_any = await!(repo.latest(&channel, &validator.id, None))
                .expect("Getting latest Message failed");

            match latest_any.expect("There was no latest message returned") {
                Message::RejectState(reject_state) => assert_eq!("my reason", reject_state.reason),
                _ => panic!("A Reject state message was not returned as latest message!"),
            }
        })
    }

    #[test]
    fn getting_latest_message_filters_by_from() {
        futures::executor::block_on(async {
            let validator = ValidatorId::try_from("identity").expect("ValidatorId failed");
            let channel = get_channel_id("channel id");

            let from =
                ValidatorId::try_from("another validator").expect("ValidatorId for form failed");
            let init_message = get_reject_memory_message(&channel, &validator, None);

            let repo = MemoryMessageRepository::new(&[init_message]);

            let result =
                await!(repo.latest(&channel, &from, None)).expect("Fetching latest message failed");

            assert!(
                result.is_none(),
                "A latest message was found, even though it has different ValidatorId"
            );
        })
    }

    #[test]
    fn getting_latest_message_filters_by_channel_id() {
        futures::executor::block_on(async {
            let validator = ValidatorId::try_from("identity").expect("ValidatorId failed");
            let channel = get_channel_id("channel 1");
            let from_channel = get_channel_id("channel 2");

            let init_message = get_reject_memory_message(&channel, &validator, None);

            let repo = MemoryMessageRepository::new(&[init_message]);

            let result = await!(repo.latest(&from_channel, &validator, None))
                .expect("Fetching latest message failed");

            assert!(
                result.is_none(),
                "A latest message was found, even though it has different ChannelId"
            );
        })
    }

    #[test]
    fn getting_latest_message_filters_by_types() {
        futures::executor::block_on(async {
            let validator = ValidatorId::try_from("identity").expect("ValidatorId failed");
            let channel = get_channel_id("channel 1");

            let init_messages = [
                get_reject_memory_message(&channel, &validator, Some("reason".to_string())),
                MemoryMessage {
                    message: Message::Heartbeat(get_heartbeat::<MemoryState>(
                        "state".to_string(),
                        "signature".to_string(),
                    )),
                    channel: channel.clone(),
                    owner: validator.clone(),
                },
            ];

            let repo = MemoryMessageRepository::new(&init_messages);

            let result = await!(repo.latest(&channel, &validator, Some(&[&TYPE_REJECT])))
                .expect("Fetching latest message failed");

            match result.expect("There was no latest message returned") {
                Message::RejectState(reject_state) => assert_eq!("reason", reject_state.reason),
                _ => panic!("Filtering by type didn't return the expected RejectState message!"),
            }
        })
    }
}
