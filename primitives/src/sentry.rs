use crate::validator::{Heartbeat, MessageTypes};
use crate::{BigNum, Channel};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_hex::{SerHex, StrictPfx};
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct LastApproved {
    /// NewState can be None if the channel is brand new
    pub new_state: Option<ValidatorMessage>,
    /// ApproveState can be None if the channel is brand new
    pub approved_state: Option<ValidatorMessage>,
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

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventAggregate {
    #[serde(with = "SerHex::<StrictPfx>")]
    pub channel_id: [u8; 32],
    pub created: DateTime<Utc>,
    pub events: HashMap<String, AggregateEvents>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AggregateEvents {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_counts: Option<HashMap<String, BigNum>>,
    pub event_payouts: HashMap<String, BigNum>,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ChannelAllResponse {
    pub channels: Vec<Channel>,
    pub total_pages: u64,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct LastApprovedResponse {
    pub last_approved: Option<LastApproved>,
    pub heartbeats: Option<Vec<Heartbeat>>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SuccessResponse {
    pub success: bool,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ValidatorMessage {
    pub from: String,
    pub received: DateTime<Utc>,
    pub msg: MessageTypes,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ValidatorMessageResponse {
    pub validator_messages: Vec<ValidatorMessage>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct EventAggregateResponse {
    pub events: Vec<EventAggregate>,
}
