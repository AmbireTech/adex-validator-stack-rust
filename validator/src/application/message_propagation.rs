use std::convert::TryFrom;
use std::error::Error;
use std::fmt;

use domain::validator::message::State;
use domain::validator::Message;
use domain::{Channel, RepositoryError, ValidatorId};

use crate::domain::MessageRepository;

pub struct MessagePropagator<S: State> {
    message_repository: Box<dyn MessageRepository<S>>,
}

#[derive(Debug)]
pub struct PropagationError {
    kind: PropagationErrorKind,
    message: String,
}

#[derive(Debug)]
pub enum PropagationErrorKind {
    Repository(RepositoryError),
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
    pub async fn propagate<'a>(
        &'a self,
        channel: &'a Channel,
        message: Message<S>,
    ) -> Vec<Result<(), PropagationError>> {
        let mut results = Vec::default();

        for validator in channel.spec.validators.into_iter() {
            // @TODO: Remove once we have ValidatorId in ValidatorDesc
            let validator_id = ValidatorId::try_from(validator.id.as_str()).unwrap();
            let add_result =
                await!(self
                    .message_repository
                    .add(&channel.id, &validator_id, message.clone()))
                .map_err(Into::into);
            results.push(add_result);
        }

        results
    }
}
