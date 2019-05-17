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

pub trait IOError: error::Error + Send {}

#[derive(Debug)]
pub enum RepositoryError {
    /// An error with the underlying implementation occurred
    IOError(Box<dyn IOError>),
    /// Error handling save errors, like Primary key already exists and etc.
    /// @TODO: Add and underlying implementation for this error
    UserError,
}

pub type RepositoryFuture<T> = Pin<Box<Future<Output=Result<T, RepositoryError>> + Send>>;
