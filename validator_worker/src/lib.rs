#![feature(async_await, await_macro)]
#![deny(rust_2018_idioms)]
#![deny(clippy::all)]
#![allow(clippy::needless_lifetimes)]

use std::error::Error;

use adapter::{get_balance_leaf, get_signable_state_root};
use primitives::adapter::Adapter;
use primitives::merkle_tree::MerkleTree;
use primitives::BalancesMap;

use crate::sentry_interface::SentryApi;

pub use self::follower::Follower;
pub use self::sentry_interface::all_channels;

pub mod error;
pub mod follower;
pub mod heartbeat;
pub mod leader;
pub mod producer;
pub mod sentry_interface;

pub mod core {
    pub mod events;
    pub mod fees;
    pub mod follower_rules;
}

pub(crate) fn get_state_root_hash<A: Adapter + 'static>(
    iface: &SentryApi<A>,
    balances: &BalancesMap,
) -> Result<[u8; 32], Box<dyn Error>> {
    // Note: MerkleTree takes care of deduplicating and sorting
    let elems: Vec<[u8; 32]> = balances
        .iter()
        .map(|(acc, amount)| get_balance_leaf(acc, amount))
        .collect::<Result<_, _>>()?;

    let tree = MerkleTree::new(&elems);

    let balance_root = hex::encode(tree.root());

    // keccak256(channelId, balanceRoot)
    get_signable_state_root(&iface.channel.id, &balance_root)
}
