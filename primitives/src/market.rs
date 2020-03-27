use chrono::serde::ts_milliseconds;
use serde::{Deserialize, Serialize};

use chrono::{DateTime, Utc};

use crate::{BalancesMap, BigNum, Channel};

// Data structs specific to the market
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum StatusType {
    Initializing,
    Ready,
    Active,
    Offline,
    Disconnected,
    Unhealthy,
    Withdraw,
    Expired,
    Exhausted,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    #[serde(rename = "name")]
    pub status_type: StatusType,
    pub usd_estimate: f32,
    #[serde(rename = "lastApprovedBalances")]
    pub balances: BalancesMap,
    #[serde(with = "ts_milliseconds")]
    pub last_checked: DateTime<Utc>,
}

impl Status {
    pub fn balances_sum(&self) -> BigNum {
        self.balances.values().sum()
    }
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Campaign {
    #[serde(flatten)]
    pub channel: Channel,
    pub status: Status,
}
