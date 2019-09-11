use std::pin::Pin;

use chrono::{DateTime, Utc};
use futures::prelude::*;
use serde::{Deserialize, Serialize};

use crate::Channel;
use crate::{BalancesMap, BigNum};

#[derive(Debug)]
pub enum ValidatorError {
    None,
    InvalidRootHash,
    InvalidSignature,
    InvalidTransition,
}

pub type ValidatorFuture<T> = Pin<Box<dyn Future<Output = Result<T, ValidatorError>> + Send>>;

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

// Validator Message Types

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Accounting {
    #[serde(rename = "last_ev_aggr")]
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
    pub balances: Option<String>,
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
