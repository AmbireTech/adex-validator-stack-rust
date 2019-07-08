use std::sync::Arc;

use futures::future::{ready, FutureExt};

use domain::validator::message::{MessageType, State};
use domain::validator::{Message, ValidatorId};
use domain::{ChannelId, RepositoryFuture};
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
    pub channel_id: ChannelId,
    pub validator_id: ValidatorId,
}

pub struct MemoryMessageRepository {
    inner: MemoryRepository<MemoryMessage, bool>,
    /// This ValidatorId will be used for the `add` method
    /// as this is usually will be handled by SentryApi and the Auth header
    self_validator_id: ValidatorId,
}

impl MemoryMessageRepository {
    pub fn new(initial_messages: &[MemoryMessage], self_validator_id: ValidatorId) -> Self {
        let cmp = Arc::new(|_message: &MemoryMessage, should_match: &bool| *should_match);

        Self {
            inner: MemoryRepository::new(&initial_messages, cmp),
            self_validator_id,
        }
    }
}

impl MessageRepository<MemoryState> for MemoryMessageRepository {
    fn add(&self, channel_id: ChannelId, message: Message<MemoryState>) -> RepositoryFuture<()> {
        let message = MemoryMessage {
            message,
            channel_id,
            validator_id: self.self_validator_id.clone(),
        };
        // this should never match against the new record, that's why always pass false.
        ready(self.inner.add(&false, message).map_err(Into::into)).boxed()
    }

    /// Fetches the latest Message of Channel from the given Validator,
    /// filtering by Types if provided.
    /// If not `types` are provided, it will match against all types.
    fn latest(
        &self,
        channel_id: &ChannelId,
        from: &ValidatorId,
        types: Option<&[&MessageType]>,
    ) -> RepositoryFuture<Option<Message<MemoryState>>> {
        let latest = self
            .inner
            .list_all(|mem_msg| {
                let is_from = &mem_msg.validator_id == from;
                let is_channel_id = &mem_msg.channel_id == channel_id;
                // if there are no types provided, it should match every type, i.e. default `true` for `None`
                let is_in_types = types.map_or(true, |message_types| {
                    mem_msg.message.is_types(message_types)
                });

                match (is_from, is_channel_id, is_in_types) {
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
    use domain::validator::message::fixtures::get_reject_state;

    use super::*;

    #[test]
    fn adds_message_with_the_self_validator_id() {
        futures::executor::block_on(async {
            let validator_id = ValidatorId::try_from("identity").expect("ValidatorId failed");
            let repo = MemoryMessageRepository::new(&[], validator_id.clone());

            // @TODO: Replace with random Message fixture fn
            let message = get_reject_state(None);
            let channel_id = get_channel_id("channel id");

            await!(repo.add(channel_id, Message::RejectState(message)))
                .expect("Adding a message failed");

            let list_all = repo
                .inner
                .list_all(|m| Some(m.clone()))
                .expect("Listing all Messages failed");

            assert_eq!(1, list_all.len());
            assert_eq!(validator_id, list_all[0].validator_id);
            assert_eq!(channel_id, list_all[0].channel_id);
        })
    }

    #[test]
    fn gets_latest_message_from_this_self_validator_id() {
        futures::executor::block_on(async {
            let validator_id = ValidatorId::try_from("identity").expect("ValidatorId failed");
            let channel_id = get_channel_id("channel id");

            let repo = MemoryMessageRepository::new(&[], validator_id.clone());
            // add an initial Reject message for checking latest ordering
            let init_message =
                Message::RejectState(get_reject_state(Some("Initial Message".to_string())));
            await!(repo.add(channel_id.clone(), init_message))
                .expect("Adding the initial message failed");

            let new_message = Message::RejectState(get_reject_state(Some("my reason".to_string())));
            await!(repo.add(channel_id.clone(), new_message)).expect("Adding a message failed");

            let latest_any = await!(repo.latest(&channel_id, &validator_id, None))
                .expect("Getting latest Message failed");

            match latest_any.expect("There was no latest message returned") {
                Message::RejectState(reject_state) => assert_eq!("my reason", reject_state.reason),
                _ => panic!("A Reject state message was not returned as latest message!"),
            }
        })
    }
}
