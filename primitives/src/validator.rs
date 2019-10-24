use std::pin::Pin;

use chrono::{DateTime, Utc};
use futures::prelude::*;
use serde::{Deserialize, Serialize};
use serde_hex::{SerHex, StrictPfx};
use std::fmt;

use crate::Channel;
use crate::{BalancesMap, BigNum, DomainError};
use std::convert::TryFrom;

#[derive(Debug)]
pub enum ValidatorError {
    None,
    InvalidRootHash,
    InvalidSignature,
    InvalidTransition,
}

pub type ValidatorFuture<T> = Pin<Box<dyn Future<Output = Result<T, ValidatorError>> + Send>>;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(transparent)]
pub struct ValidatorId(#[serde(with = "SerHex::<StrictPfx>")] [u8; 20]);

impl ValidatorId {
    pub fn to_hex_prefix_string(&self) -> String {
        format!("0x{}", hex::encode(self.0))
    }

    pub fn into_inner(&self) -> &[u8; 20] {
        &self.0
    }

    pub fn to_hex_checksummed_string(&self) -> String {
        eth_checksum::checksum(&format!("0x{}", hex::encode(self.0)))
    }
}

impl TryFrom<&str> for ValidatorId {
    type Error = DomainError;
    // 0x prefixed string
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        // use hex::FromHex;
        // @TODO: Should we have some constrains(like valid hex string starting with `0x`)? If not this should be just `From`.
        let mut hex_value = value;
        if value.len() == 42 {
            hex_value = &value[2..];
        }
        let result = hex::decode(hex_value).map_err(|_| {
            DomainError::InvalidArgument("Failed to deserialize validator id".to_string())
        })?;
        let mut id: [u8; 20] = [0; 20];
        id.copy_from_slice(&result[..]);
        Ok(Self(id))
    }
}

impl TryFrom<String> for ValidatorId {
    type Error = DomainError;
    // 0x prefixed string
    fn try_from(value: String) -> Result<Self, Self::Error> {
        println!("value {}", value);
        // use hex::FromHex;
        // @TODO: Should we have some constrains(like valid hex string starting with `0x`)? If not this should be just `From`.
        let result = hex::decode(&value[2..]).map_err(|_| {
            DomainError::InvalidArgument("Failed to deserialize validator id".to_string())
        })?;
        let mut id: [u8; 20] = [0; 20];
        id.copy_from_slice(&result[..]);
        Ok(Self(id))
    }
}
// returns a 0x prefix string
impl Into<String> for ValidatorId {
    fn into(self) -> String {
        format!("{}", hex::encode(self.0))
    }
}

impl fmt::Display for ValidatorId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", format!("0x{}", hex::encode(self.0)))
    }
}

pub trait Validator {
    fn tick(&self, channel: Channel) -> ValidatorFuture<()>;
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ValidatorDesc {
    pub id: ValidatorId,
    pub url: String,
    pub fee: BigNum,
}

// Validator Message Types

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Accounting {
    #[serde(rename = "lastEvAggr")]
    pub last_event_aggregate: DateTime<Utc>,
    pub balances_before_fees: BalancesMap,
    pub balances: BalancesMap,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ApproveState {
    pub state_root: String,
    pub signature: String,
    pub is_healthy: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct NewState {
    pub state_root: String,
    pub signature: String,
    pub balances: BalancesMap,
}

#[derive(Default, Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RejectState {
    pub reason: String,
    pub state_root: String,
    pub signature: String,
    pub balances: Option<BalancesMap>,
    pub timestamp: Option<DateTime<Utc>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Heartbeat {
    pub signature: String,
    pub state_root: String,
    pub timestamp: DateTime<Utc>,
}

impl Heartbeat {
    pub fn new(signature: String, state_root: String) -> Self {
        Self {
            signature,
            state_root,
            timestamp: Utc::now(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum MessageTypes {
    ApproveState(ApproveState),
    NewState(NewState),
    RejectState(RejectState),
    Heartbeat(Heartbeat),
    Accounting(Accounting),
}
