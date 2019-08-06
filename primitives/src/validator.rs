use std::convert::TryFrom;
use std::fmt;

use serde::{Deserialize, Serialize};

pub use message::Message;

use crate::{BigNum, DomainError};

pub mod message;

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
        fn add(
            &self,
            channel: &ChannelId,
            validator: &ValidatorId,
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

impl Into<String> for ValidatorId {
    fn into(self) -> String {
        self.0
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

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ValidatorDesc {
    pub id: ValidatorId,
    pub url: String,
    pub fee: BigNum,
}

#[cfg(any(test, feature = "fixtures"))]
pub mod fixtures {
    use fake::faker::*;

    use super::{ValidatorDesc, ValidatorId};
    use crate::BigNum;
    use std::convert::TryFrom;

    pub fn get_validator<V: AsRef<str>>(validator_id: V, fee: Option<BigNum>) -> ValidatorDesc {
        let fee = fee.unwrap_or_else(|| BigNum::from(<Faker as Number>::between(1, 13)));
        let url = format!(
            "http://{}-validator-url.com/validator",
            validator_id.as_ref()
        );
        let validator_id =
            ValidatorId::try_from(validator_id.as_ref()).expect("Creating ValidatorId failed");

        ValidatorDesc {
            id: validator_id,
            url,
            fee,
        }
    }

    pub fn get_validators(count: usize, prefix: Option<&str>) -> Vec<ValidatorDesc> {
        let prefix = prefix.map_or(String::new(), |prefix| format!("{}-", prefix));
        (0..count)
            .map(|c| {
                let validator_id = format!("{}validator-{}", prefix, c + 1);

                get_validator(&validator_id, None)
            })
            .collect()
    }
}
