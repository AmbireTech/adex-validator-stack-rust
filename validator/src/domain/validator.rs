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
    use domain::validator::message::{Message, MessageType, State};
    use domain::{ChannelId, RepositoryFuture, ValidatorDesc, ValidatorId};

    pub trait MessageRepository<S: State> {
        /// Adds a Message to the passed Validator
        /// Accepts ValidatorDesc instead of ValidatorId, as we need to know the Validator Url as well
        fn add(
            &self,
            for_channel: &ChannelId,
            to_validator: &ValidatorDesc,
            message: Message<S>,
        ) -> RepositoryFuture<()>;

        fn latest(
            &self,
            channel: &ChannelId,
            from: &ValidatorId,
            types: Option<&[&MessageType]>,
        ) -> RepositoryFuture<Option<Message<S>>>;
    }
}
