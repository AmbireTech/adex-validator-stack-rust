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
    use domain::validator::message::State;
    use domain::validator::Message;
    use domain::RepositoryFuture;

    pub trait MessageRepository {
        fn add<S: State>(message: Message<S>) -> RepositoryFuture<()>;
    }
}
