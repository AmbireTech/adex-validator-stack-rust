use std::error;
use std::fmt;

#[cfg(any(test, feature = "fixtures"))]
pub use util::tests as test_util;

pub use self::ad_unit::AdUnit;
pub use self::asset::Asset;
pub use self::bignum::BigNum;
#[cfg(feature = "repositories")]
pub use self::channel::ChannelRepository;
pub use self::channel::{Channel, ChannelId, ChannelListParams, ChannelSpec};
pub use self::event_submission::EventSubmission;
#[cfg(feature = "repositories")]
pub use self::repository::*;
pub use self::targeting_tag::TargetingTag;
pub use self::validator::ValidatorDesc;

pub mod ad_unit;
pub mod asset;
pub mod bignum;
pub mod channel;
pub mod event_submission;
pub mod targeting_tag;
pub mod util;
pub mod validator;

/// re-exports all the fixtures in one module
#[cfg(any(test, feature = "fixtures"))]
pub mod fixtures {
    pub use super::asset::fixtures::*;
    pub use super::channel::fixtures::*;
    pub use super::targeting_tag::fixtures::*;
    pub use super::validator::fixtures::*;
}

#[derive(Debug)]
pub enum DomainError {
    InvalidArgument(String),
}

impl fmt::Display for DomainError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Domain error",)
    }
}

impl error::Error for DomainError {
    fn cause(&self) -> Option<&error::Error> {
        None
    }
}

#[cfg(feature = "repositories")]
pub mod repository {
    use std::pin::Pin;

    use futures::Future;

    pub trait IOError: std::error::Error + Send {}

    #[derive(Debug)]
    pub enum RepositoryError {
        /// An error with the underlying implementation occurred
        IO(Box<dyn IOError>),
        /// Error handling save errors, like Primary key already exists and etc.
        /// @TODO: Add and underlying implementation for this error
        User,
    }

    pub type RepositoryFuture<T> = Pin<Box<Future<Output = Result<T, RepositoryError>> + Send>>;
}
