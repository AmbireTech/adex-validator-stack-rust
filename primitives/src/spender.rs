use crate::{Address, BalancesMap, UnifiedNum, channel_v5::Channel};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Deposit {
    pub total: UnifiedNum,
    pub still_on_create2: UnifiedNum,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Spendable {
    pub spender: Address,
    pub channel: Channel,
    #[serde(flatten)]
    pub deposit: Deposit,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Aggregate {
    pub spender: Address,
    pub channel: Channel,
    pub balances: BalancesMap,
    pub created: DateTime<Utc>,
}
