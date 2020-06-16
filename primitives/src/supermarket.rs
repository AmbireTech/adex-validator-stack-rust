use crate::{BalancesMap, Channel};

#[derive(Debug, Clone)]
pub struct Campaign {
    channel: Channel,
    status: Status,
    balances: BalancesMap,
}

impl Campaign {
    pub fn new(channel: Channel, status: Status, balances: BalancesMap) -> Self {
        Self {
            channel,
            status,
            balances,
        }
    }
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub enum Status {
    // Active and Ready
    Active,
    Pending,
    Initializing,
    Waiting,
    Finalized(Finalized),
    Unsound {
        disconnected: bool,
        offline: bool,
        rejected_state: bool,
        unhealthy: bool,
    },
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Finalized {
    Expired,
    Exhausted,
    Withdraw,
}
