use std::error::Error;
use std::fmt;

use domain::validator::message::{Message, State};
use domain::{Channel, RepositoryError};

use crate::domain::MessageRepository;

pub struct MessagePropagator<S: State> {
    pub message_repository: Box<dyn MessageRepository<S>>,
}

#[derive(Debug)]
pub enum PropagationErrorKind {
    Repository(RepositoryError),
}

#[derive(Debug)]
pub struct PropagationError {
    kind: PropagationErrorKind,
    message: String,
}

impl fmt::Display for PropagationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl Error for PropagationError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match &self.kind {
            PropagationErrorKind::Repository(error) => Some(error),
        }
    }
}

impl From<RepositoryError> for PropagationError {
    fn from(error: RepositoryError) -> Self {
        Self {
            kind: PropagationErrorKind::Repository(error),
            message: "Repository call for propagating the message failed".to_string(),
        }
    }
}

impl<S: State> MessagePropagator<S> {
    // @TODO: Make sure we have information for logging the results for particular Validator
    pub async fn propagate<'a>(
        &'a self,
        channel: &'a Channel,
        message: Message<S>,
    ) -> Vec<Result<(), PropagationError>> {
        let mut results = Vec::default();

        for validator in channel.spec.validators.into_iter() {
            let add_result =
                await!(self
                    .message_repository
                    .add(&channel.id, &validator, message.clone()))
                .map_err(Into::into);
            results.push(add_result);
        }

        results
    }
}

#[cfg(test)]
mod test {
    use std::cell::RefCell;

    use futures::future::{ready, FutureExt};

    use domain::channel::fixtures::get_channel;
    use domain::validator::message::fixtures::{get_reject_state, DummyState};
    use domain::validator::message::{Message, MessageType};
    use domain::{ChannelId, ValidatorDesc, ValidatorId};
    use domain::{RepositoryError, RepositoryFuture};

    use crate::application::MessagePropagator;
    use crate::domain::MessageRepository;

    struct MockMessageRepository<I>
    where
        I: Iterator<Item = Result<(), RepositoryError>>,
    {
        add_results: RefCell<I>,
    }

    impl<I> MessageRepository<DummyState> for MockMessageRepository<I>
    where
        I: Iterator<Item = Result<(), RepositoryError>>,
    {
        fn add(
            &self,
            _channel: &ChannelId,
            _validator: &ValidatorDesc,
            _message: Message<DummyState>,
        ) -> RepositoryFuture<()> {
            let result = self
                .add_results
                .borrow_mut()
                .next()
                .expect("Whoops, you called add() more than the provided results");
            ready(result).boxed()
        }

        fn latest(
            &self,
            _channel: &ChannelId,
            _from: &ValidatorId,
            _types: Option<&[&MessageType]>,
        ) -> RepositoryFuture<Option<Message<DummyState>>> {
            unimplemented!("No need for latest in this Mock")
        }
    }

    #[test]
    fn propagates_and_returns_vector_of_results() {
        futures::executor::block_on(async {
            let add_error = RepositoryError::User;

            let iterator = vec![Ok(()), Err(add_error)].into_iter();
            let message_repository = MockMessageRepository {
                add_results: RefCell::new(iterator),
            };
            let propagator = MessagePropagator {
                message_repository: Box::new(message_repository),
            };

            let message = get_reject_state(None);
            let channel = get_channel("id", &None, None);

            let result = await!(propagator.propagate(&channel, Message::RejectState(message)));

            assert_eq!(2, result.len());
            assert!(result[0].is_ok());
            match &result[1] {
                Ok(_) => panic!("It should be an error"),
                Err(error) => assert_eq!(
                    "Repository call for propagating the message failed",
                    error.message
                ),
            }
        })
    }
}
