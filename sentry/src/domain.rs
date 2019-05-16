use std::error;
use std::fmt;
use std::pin::Pin;

use futures::Future;

pub use bignum::BigNum;
pub use channel::{Channel, ChannelRepository, ChannelSpec};
pub use validator::ValidatorDesc;

mod bignum;
mod channel;
mod validator;

#[derive(Debug)]
pub struct DomainError;

impl fmt::Display for DomainError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Domain error", )
    }
}

impl error::Error for DomainError {
    fn cause(&self) -> Option<&error::Error> {
        None
    }
}

#[derive(Debug)]
pub enum RepositoryError {
    PersistenceError(Box<dyn error::Error + Send>),
    AlreadyExists,
}

pub type RepositoryFuture<T> = Pin<Box<Future<Output=Result<T, RepositoryError>> + Send>>;
