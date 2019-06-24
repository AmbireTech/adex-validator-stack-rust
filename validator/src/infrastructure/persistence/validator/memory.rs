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
        channel_id: ChannelId,
        from: ValidatorId,
        types: Option<&[&MessageType]>,
    ) -> RepositoryFuture<Option<Message<MemoryState>>> {
        let latest = self
            .inner
            .list_all(|mem_msg| {
                let is_from = mem_msg.validator_id == from;
                let is_channel_id = mem_msg.channel_id == channel_id;
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
