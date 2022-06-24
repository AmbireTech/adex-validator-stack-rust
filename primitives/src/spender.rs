use crate::{Address, Channel, Deposit, UnifiedNum};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Spender {
    pub total_deposited: UnifiedNum,
    pub total_spent: Option<UnifiedNum>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spendable {
    pub spender: Address,
    pub channel: Channel,
    pub deposit: Deposit<UnifiedNum>,
}

impl PartialEq<Spendable> for &Spendable {
    fn eq(&self, other: &Spendable) -> bool {
        self.spender == other.spender
            && self.channel == other.channel
            && self.deposit == other.deposit
    }
}

impl PartialEq<&Spendable> for Spendable {
    fn eq(&self, other: &&Spendable) -> bool {
        self.spender == other.spender
            && self.channel == other.channel
            && self.deposit == other.deposit
    }
}

#[cfg(feature = "postgres")]
mod postgres {
    use super::*;
    use tokio_postgres::Row;

    impl From<&Row> for Spendable {
        fn from(row: &Row) -> Self {
            Self {
                spender: row.get("spender"),
                channel: Channel::from(row),
                deposit: Deposit {
                    total: row.get("total"),
                },
            }
        }
    }
}
