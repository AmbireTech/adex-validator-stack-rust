use std::sync::Arc;

use futures::future::{ready, FutureExt};

use domain::validator::message::State;
use domain::validator::Message;
use domain::RepositoryFuture;
use memory_repository::MemoryRepository;

use crate::domain::validator::repository::MessageRepository;

#[derive(Clone)]
pub struct MemoryState {}

impl State for MemoryState {
    type Signature = String;
    type StateRoot = String;
}

pub struct MemoryMessageRepository {
    inner: MemoryRepository<Message<MemoryState>, bool>,
}

impl MemoryMessageRepository {
    pub fn new(initial_messages: &[Message<MemoryState>]) -> Self {
        let cmp = Arc::new(|_message: &Message<MemoryState>, should_match: &bool| *should_match);
        Self {
            inner: MemoryRepository::new(&initial_messages, cmp),
        }
    }
}

impl MessageRepository<MemoryState> for MemoryMessageRepository {
    fn add(&self, message: Message<MemoryState>) -> RepositoryFuture<()> {
        // this should never match against the new record, that's why always pass false.
        ready(self.inner.add(&false, message).map_err(Into::into)).boxed()
    }
}
