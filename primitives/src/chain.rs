use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fmt};

pub static CHAINS: Lazy<HashMap<ChainId, Chain>> = Lazy::new(|| {
    let mut map = HashMap::new();
    map.insert(
        ChainId(1),
        Chain {
            chain_id: ChainId(1),
            name: "Ethereum Mainnet",
            short: "eth",
            network: "mainnet",
        },
    );

    map.insert(
        ChainId(5),
        Chain {
            chain_id: ChainId(5),
            name: "Ethereum Testnet Görli",
            short: "gor",
            network: "goerli",
        },
    );

    map.insert(
        ChainId(100),
        Chain {
            chain_id: ChainId(100),
            name: "xDAI Chain",
            short: "xdai",
            network: "mainnet",
        },
    );

    map
});

/// Ethereum Virtual Machine Chain
/// see https://chainid.network
pub struct Chain {
    pub chain_id: ChainId,
    pub name: &'static str,
    pub short: &'static str,
    pub network: &'static str,
}

#[derive(Serialize, Deserialize, Hash, Clone, Copy, Eq, PartialEq)]
#[serde(transparent)]
pub struct ChainId(u32);

impl ChainId {
    pub fn new(id: u32) -> Self {
        Self(id)
    }
}

/// Default ChainId: 1 - Ethereum Mainnet
pub fn eth_mainnet() -> ChainId {
    ChainId(1)
}

impl fmt::Debug for ChainId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ChainId({})", self.0)
    }
}
