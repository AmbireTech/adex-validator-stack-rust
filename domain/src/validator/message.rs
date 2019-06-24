use std::fmt;

use chrono::{DateTime, Utc};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::BalancesMap;

pub trait State {
    type Signature: DeserializeOwned + Serialize + fmt::Debug + Clone;
    type StateRoot: DeserializeOwned + Serialize + fmt::Debug + Clone;
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum Message<S: State> {
    ApproveState(ApproveState<S>),
    NewState(NewState<S>),
    RejectState(RejectState),
    Heartbeat(Heartbeat<S>),
    Accounting(Accounting),
}

impl<S: State> Message<S> {
    pub fn is_type(&self, message_type: &MessageType) -> bool {
        assert!(ALL_TYPES.contains(&message_type));

        let self_message_type = match self {
            Message::ApproveState(_) => &TYPE_APPROVE,
            Message::NewState(_) => &TYPE_NEW,
            Message::RejectState(_) => &TYPE_REJECT,
            Message::Heartbeat(_) => &TYPE_HEARTBEAT,
            Message::Accounting(_) => &TYPE_ACCOUNTING,
        };

        self_message_type == message_type
    }

    pub fn is_types(&self, types: &[&MessageType]) -> bool {
        types.iter().any(|&m_type| self.is_type(m_type))
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct MessageType(&'static str);

pub const TYPE_APPROVE: MessageType = MessageType("approve");
pub const TYPE_NEW: MessageType = MessageType("new");
pub const TYPE_REJECT: MessageType = MessageType("reject");
pub const TYPE_HEARTBEAT: MessageType = MessageType("heartbeat");
pub const TYPE_ACCOUNTING: MessageType = MessageType("accounting");
pub const ALL_TYPES: [&MessageType; 5] = [
    &TYPE_APPROVE,
    &TYPE_NEW,
    &TYPE_REJECT,
    &TYPE_HEARTBEAT,
    &TYPE_ACCOUNTING,
];

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ApproveState<S: State> {
    state_root: S::StateRoot,
    signature: S::Signature,
    is_healthy: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct NewState<S: State> {
    state_root: S::StateRoot,
    signature: S::Signature,
    balances: BalancesMap,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RejectState {
    reason: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Heartbeat<S: State> {
    signature: S::Signature,
    state_root: S::StateRoot,
    timestamp: DateTime<Utc>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
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
