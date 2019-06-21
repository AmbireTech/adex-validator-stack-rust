use chrono::{DateTime, Utc};
use serde::de::DeserializeOwned;
use serde::export::fmt::Debug;
use serde::{Deserialize, Serialize};

use crate::BalancesMap;

pub trait State {
    type Signature: DeserializeOwned + Serialize + Debug;
    type StateRoot: DeserializeOwned + Serialize + Debug;
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(tag = "type")]
pub enum Message<S: State> {
    ApproveState(ApproveState<S>),
    NewState(NewState<S>),
    RejectState(RejectState),
    Heartbeat(Heartbeat<S>),
    Accounting(Accounting),
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ApproveState<S: State> {
    state_root: S::StateRoot,
    signature: S::Signature,
    is_healthy: bool,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct NewState<S: State> {
    state_root: S::StateRoot,
    signature: S::Signature,
    balances: BalancesMap,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct RejectState {
    reason: String,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Heartbeat<S: State> {
    signature: S::Signature,
    timestamp: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Accounting {
    #[serde(rename = "last_ev_aggr")]
    last_event_aggregate: DateTime<Utc>,
    #[serde(rename = "balances_pre_fees")]
    pre_fees: BalancesMap,
    balances: BalancesMap,
}

#[cfg(any(test, feature = "fixtures"))]
pub mod fixtures {
    use fake::faker::*;

    use crate::test_util::time::past_datetime;

    use super::*;

    #[derive(Serialize, Deserialize, Debug)]
    pub struct DummyState {}

    impl State for DummyState {
        type Signature = String;
        type StateRoot = String;
    }

    pub fn get_approve_state<S: State>(
        state_root: S::StateRoot,
        signature: S::Signature,
        is_healthy: bool,
    ) -> ApproveState<S> {
        ApproveState {
            state_root,
            signature,
            is_healthy,
        }
    }

    pub fn get_new_state<S: State>(
        state_root: S::StateRoot,
        signature: S::Signature,
        balances: BalancesMap,
    ) -> NewState<S> {
        NewState {
            state_root,
            signature,
            balances,
        }
    }

    pub fn get_reject_state(reason: Option<String>) -> RejectState {
        RejectState {
            reason: reason.unwrap_or_else(|| <Faker as Lorem>::sentence(5, 4)),
        }
    }

    pub fn get_heartbeat<S: State>(signature: S::Signature) -> Heartbeat<S> {
        Heartbeat {
            signature,
            timestamp: past_datetime(None),
        }
    }

    pub fn get_accounting(
        balances: BalancesMap,
        pre_fees: Option<BalancesMap>,
        last_ev_aggr: Option<DateTime<Utc>>,
    ) -> Accounting {
        let last_event_aggregate = last_ev_aggr.unwrap_or_else(|| past_datetime(None));
        assert!(
            last_event_aggregate < Utc::now(),
            "You cannot have a last_event_aggregate < Now"
        );

        Accounting {
            last_event_aggregate,
            pre_fees: pre_fees.unwrap_or_else(BalancesMap::default),
            balances,
        }
    }
}
