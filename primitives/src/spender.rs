use crate::{channel_v5::Channel, Address, BalancesMap, UnifiedNum};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio_postgres::Row;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Deposit {
    pub total: UnifiedNum,
    pub still_on_create2: UnifiedNum,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
#[cfg(feature = "postgres")]
mod postgres {
    use super::*;


    impl From<Row> for Spendable {
        fn from(row: Row) -> Self {
            Self {
                spender: row.get("spender"),
                channel: row.get("channel"),
                deposit: Deposit {
                    total: row.get("total"),
                    still_on_create2: row.get("still_on_create2")
                },
            }
        }
    }
}
