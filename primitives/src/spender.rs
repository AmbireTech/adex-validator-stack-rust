use crate::{channel_v5::Channel, Address, BalancesMap, UnifiedNum};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Deposit {
    pub total: UnifiedNum,
    pub still_on_create2: UnifiedNum,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SpenderLeaf {
    pub total_spent: UnifiedNum,
    // merkle_proof: [u8; 32], // TODO
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Spender {
    pub total_deposited: UnifiedNum,
    pub spender_leaf: Option<SpenderLeaf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spendable {
    pub spender: Address,
    pub channel: Channel,
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
#[cfg(feature = "postgres")]
mod postgres {
    use super::*;
    use tokio_postgres::Row;

    impl From<Row> for Spendable {
        fn from(row: Row) -> Self {
            Self {
                spender: row.get("spender"),
                channel: row.get("channel"),
                deposit: Deposit {
                    total: row.get("total"),
                    still_on_create2: row.get("still_on_create2"),
                },
            }
        }
    }
}
