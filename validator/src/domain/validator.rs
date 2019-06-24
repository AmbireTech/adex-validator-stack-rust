use std::pin::Pin;

use futures::Future;

use domain::Channel;

pub use self::repository::MessageRepository;

pub type ValidatorFuture<T> = Pin<Box<dyn Future<Output = Result<T, ValidatorError>> + Send>>;

#[derive(Debug)]
pub enum ValidatorError {
    None,
}

pub trait Validator {
    fn tick(&self, channel: Channel) -> ValidatorFuture<()>;
}

pub mod repository {
    use domain::validator::message::{MessageType, State};
    use domain::validator::{Message, ValidatorId};
    use domain::{ChannelId, RepositoryFuture};

    pub trait MessageRepository<S: State> {
        fn add(&self, channel_id: ChannelId, message: Message<S>) -> RepositoryFuture<()>;

        fn latest(
            &self,
            channel_id: ChannelId,
            from: ValidatorId,
            types: Option<&[&MessageType]>,
        ) -> RepositoryFuture<Option<Message<S>>>;
    }
}
