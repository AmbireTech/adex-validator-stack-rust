use std::convert::TryFrom;
use std::fmt;
//
use serde::{Deserialize, Serialize};
use crate::{ BigNum, BalancesMap };
use chrono::{DateTime, Utc};
use std::pin::Pin;
use futures::prelude::*;
use crate::Channel;

pub type ValidatorFuture<T> = Pin<Box<dyn Future<Output = Result<T, ValidatorError>> + Send>>;

#[derive(Debug)]
pub enum ValidatorError {
    None,
    InvalidRootHash,
    InvalidSignature,
    InvalidTransition
}

pub trait Validator {
    fn tick(&self, channel: Channel) -> ValidatorFuture<()>;
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ValidatorDesc {
    pub id: String,
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


// Validator Message Types

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Accounting {
    #[serde(rename = "type")]
    pub message_type: String,
    #[serde(rename = "last_ev_aggr")]
    pub last_event_aggregate: DateTime<Utc>,
    pub balances_before_fees: BalancesMap,
    pub balances: BalancesMap,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ApproveState {
    #[serde(rename = "type")]
    pub message_type: String,
    pub state_root: String,
    pub signature: String,
    pub is_healthy: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct NewState {
    #[serde(rename = "type")]
    pub message_type: String,
    pub state_root: String,
    pub signature: String,
    pub balances: String,
}

#[derive(Default, Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RejectState {
    #[serde(rename = "type")]
    pub message_type: String,
    pub reason: String,
    pub state_root: String,
    pub signature: String,
    pub balances: Option<String>,
    pub timestamp: Option<DateTime<Utc>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Heartbeat  {
    #[serde(rename = "type")]
    pub message_type: String,
    pub signature: String,
    pub state_root: String,
    pub timestamp: DateTime<Utc>,
    // we always want to create heartbeat with Timestamp NOW, so add a hidden field
    // and force the creation of Heartbeat always to be from the `new()` method
    _secret: (),
}


impl Heartbeat {
    pub fn new(signature: String, state_root: String) -> Self {
        Self {
            message_type: "Heartbeat".into(),
            signature,
            state_root,
            timestamp: Utc::now(),
            _secret: (),
        }
    }
}

pub enum MessageTypes {
    ApproveState(ApproveState),
    NewState(NewState),
    RejectState(RejectState),
    Heartbeat(Heartbeat),
    Accounting(Accounting),
}




>>>>>>> 59ef6ec... refactor: validator message types
