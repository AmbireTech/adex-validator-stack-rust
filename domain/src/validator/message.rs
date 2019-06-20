use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::BalancesMap;
use serde::de::DeserializeOwned;
use serde::export::fmt::Debug;

pub trait State {
    type Signature: DeserializeOwned + Serialize + Debug;
    type StateRoot: DeserializeOwned + Serialize + Debug;
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
pub enum Message<S: State> {
    #[serde(rename_all = "camelCase")]
    ApproveState(ApproveState<S>),
    #[serde(rename_all = "camelCase")]
    NewState(NewState<S>),
    #[serde(rename_all = "camelCase")]
    Heartbeat(Heartbeat<S>),
    #[serde(rename_all = "camelCase")]
    Accounting(Accounting),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ApproveState<S: State> {
    state_root: S::StateRoot,
    signature: S::Signature,
    is_healthy: bool,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct NewState<S: State> {
    state_root: S::StateRoot,
    signature: S::Signature,
    balances: BalancesMap,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct RejectState {
    reason: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Heartbeat<S: State> {
    signature: S::Signature,
    timestamp: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Accounting {
    #[serde(rename = "last_ev_aggr")]
    last_event_aggregate: DateTime<Utc>,
    #[serde(rename = "balances_pre_fees")]
    pre_fees: BalancesMap,
    balances: BalancesMap,
}

#[cfg(any(test, feature = "fixtures"))]
pub mod fixtures {
    use crate::test_util::time::past_datetime;

    use super::*;

    pub fn get_approve_state<S: State>() -> ApproveState<S> {
        unimplemented!()
    }

    pub fn get_new_state<S: State>() -> NewState<S> {
        unimplemented!()
    }

    pub fn get_reject_state() -> RejectState {
        unimplemented!()
    }

    pub fn get_heartbeat<S: State>() -> Heartbeat<S> {
        unimplemented!()
    }

    pub fn get_accounting() -> Accounting {
        Accounting {
            last_event_aggregate: past_datetime(None),
            pre_fees: Default::default(),
            balances: Default::default(),
        }
    }
}
