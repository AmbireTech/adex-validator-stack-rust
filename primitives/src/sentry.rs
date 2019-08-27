use chrono::{DateTime, Utc};
use hex::FromHex;
use serde::{Deserialize, Serialize};
use crate::{Channel, BalancesMap};

pub trait SentryInterface {
    fn propagate() -> bool;
    fn get_latest_msg -> None;
    fn get_our_latest_msg -> None;
    fn get_last_approve -> None;
    fn get_last_msgs ->None;
    fn get_event_aggrs -> None;
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ValidatorMessage {
    type: String,
    state_root: String,
    signature: String,
    last_ev_aggr: String,
    balances: BalancesMap,
    timestamp: String,
    balances_before_fees: BalancesMap,
    reason: String,
    is_healthy: bool
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SentryValidatorMessage {
    from: String,
    received: Datetime<Utc>,
    msg: Vec<ValidatorMessage>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct LastApproved {
    new_state: ValidatorMessage,
    approved_state: ValidatorMessage,
}

#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
#[derive(Serialize, Deserialize)]
pub enum Event {
    #[serde(rename_all = "camelCase")]
    Impression {
        publisher: String,
        ad_unit: Option<String>,
    },
    ImpressionWithCommission {
        earners: Vec<Earner>,
    },
    /// only the creator can send this event
    UpdateImpressionPrice {
        price: BigNum,
    },
    /// only the creator can send this event
    Pay {
        outputs: HashMap<String, BigNum>,
    },
    /// only the creator can send this event
    PauseChannel,
    /// only the creator can send this event
    Close,
}

#[derive(Serialize, Deserialize)]
pub struct Earner {
    #[serde(rename = "publisher")]
    pub address: String,
    pub promilles: u64,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventAggregate {
    pub channel_id: ChannelId,
    pub created: DateTime<Utc>,
    pub events: HashMap<String, AggregateEvents>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AggregateEvents {
    pub event_counts: HashMap<String, BigNum>,
    pub event_payouts: HashMap<String, BigNum>,
}
