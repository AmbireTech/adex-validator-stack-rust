use domain::validator::message::State;
use domain::validator::Message;
use domain::RepositoryFuture;

use crate::domain::validator::repository::MessageRepository;

pub struct MemoryValidatorRepository {}

impl MessageRepository for MemoryValidatorRepository {
    fn add<S: State>(&self, _message: Message<S>) -> RepositoryFuture<()> {
        unimplemented!()
    }
}
