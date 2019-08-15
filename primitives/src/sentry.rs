use chrono::{DateTime, Utc};
use hex::FromHex;
use serde::{Deserialize, Serialize};
use crate::{Channel};

pub trait SentryInterface {
    fn propagate() -> bool;
    fn get_latest_msg -> None;
    fn get_our_latest_msg -> None;
    fn get_last_approve -> None;
    fn get_last_msgs ->None;
    fn get_event_aggrs -> None;
}


#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventAggregateResponse {
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
