use std::convert::TryFrom;
use std::fmt;
//
use serde::{Deserialize, Serialize};
//
//pub use message::Message;
//
use crate::{ BigNum };
//
//pub mod message;
//
use std::pin::Pin;
//
use futures::Future;
//
use crate::Channel;
//
//pub use self::repository::MessageRepository;
//
pub type ValidatorFuture<T> = Pin<Box<dyn Future<Output = Result<T, ValidatorError>> + Send>>;
//
#[derive(Debug)]
pub enum ValidatorError {
    None,
}
//
pub trait Validator {
    fn tick(&self, channel: Channel) -> ValidatorFuture<()>;
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ValidatorDesc {
    // @TODO: Replace id `String` with `ValidatorId` https://github.com/AdExNetwork/adex-validator-stack-rust/issues/83
    pub id: ValidatorId,
    pub url: String,
    pub fee: BigNum,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(transparent)]
pub struct ValidatorId(String);

impl TryFrom<&str> for ValidatorId {
    type Error = DomainError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        // @TODO: Should we have some constrains(like valid hex string starting with `0x`)? If not this should be just `From`.
        Ok(Self(value.to_string()))
    }
}

impl AsRef<str> for ValidatorId {
    fn as_ref(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Display for ValidatorId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}


//
//pub mod repository {
//    use domain::validator::message::{MessageType, State};
//    use domain::validator::{Message, ValidatorId};
//    use domain::{ChannelId, RepositoryFuture};
//
//    pub trait MessageRepository<S: State> {
//        fn add(
//            &self,
//            channel: &ChannelId,
//            validator: &ValidatorId,
//            message: Message<S>,
//        ) -> RepositoryFuture<()>;
//
//        fn latest(
//            &self,
//            channel: &ChannelId,
//            from: &ValidatorId,
//            types: Option<&[&MessageType]>,
//        ) -> RepositoryFuture<Option<Message<S>>>;
//    }
//}
//

