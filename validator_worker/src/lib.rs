#![deny(rust_2018_idioms)]
#![deny(clippy::all)]
#![deny(rustdoc::broken_intra_doc_links)]
#![cfg_attr(docsrs, feature(doc_cfg))]

use adapter::util::{get_balance_leaf, get_signable_state_root, BalanceLeafError};
use primitives::{
    balances::CheckedState,
    merkle_tree::{Error as MerkleTreeError, MerkleTree},
    Balances, ChannelId,
};
use thiserror::Error;

#[doc(inline)]
pub use self::sentry_interface::SentryApi;
#[doc(inline)]
pub use worker::Worker;

pub mod channel;
pub mod error;
pub mod follower;
pub mod heartbeat;
pub mod leader;
pub mod sentry_interface;
pub mod worker;

pub mod core {
    pub mod follower_rules;
}

#[derive(Debug, Error)]
pub enum GetStateRootError {
    #[error("Failed to get balance leaf")]
    BalanceLeaf(#[from] BalanceLeafError),
    #[error(transparent)]
    MerkleTree(#[from] MerkleTreeError),
}

pub trait GetStateRoot {
    /// Hashes the struct to produce a StateRoot `[u8; 32]`
    fn hash(&self, channel: ChannelId, token_precision: u8) -> Result<[u8; 32], GetStateRootError>;

    /// Calls `hash()` and then `hex::encode`s the result ready to be used in a Validator Message
    fn encode(&self, channel: ChannelId, token_precision: u8) -> Result<String, GetStateRootError> {
        self.hash(channel, token_precision).map(hex::encode)
    }
}

impl GetStateRoot for Balances<CheckedState> {
    fn hash(&self, channel: ChannelId, token_precision: u8) -> Result<[u8; 32], GetStateRootError> {
        get_state_root_hash(channel, self, token_precision)
    }
}

fn get_state_root_hash(
    channel: ChannelId,
    balances: &Balances<CheckedState>,
    token_precision: u8,
) -> Result<[u8; 32], GetStateRootError> {
    let spenders = balances.spenders.iter().map(|(address, amount)| {
        get_balance_leaf(true, address, &amount.to_precision(token_precision))
    });

    // Note: MerkleTree takes care of deduplicating and sorting
    let elems: Vec<[u8; 32]> = balances
        .earners
        .iter()
        .map(|(acc, amount)| get_balance_leaf(false, acc, &amount.to_precision(token_precision)))
        .chain(spenders)
        .collect::<Result<_, _>>()?;

    let tree = MerkleTree::new(&elems)?;
    // keccak256(channelId, balanceRoot)
    Ok(get_signable_state_root(channel.as_ref(), &tree.root()))
}

#[cfg(test)]
mod test {
    use super::*;

    use primitives::{
        channel::Nonce,
        test_util::{CREATOR, FOLLOWER, GUARDIAN, IDS, LEADER, PUBLISHER},
        Channel,
    };

    #[test]
    fn get_state_root_hash_returns_correct_hash() {
        let channel = Channel {
            leader: IDS[&LEADER],
            follower: IDS[&FOLLOWER],
            guardian: IDS[&GUARDIAN].to_address(),
            // DAI on goerli
            token: "0x73967c6a0904aa032c103b4104747e88c566b1a2"
                .parse()
                .expect("Valid DAI token address"),
            nonce: Nonce::from(987_654_321_u32),
        };

        let mut balances = Balances::<CheckedState>::default();

        balances
            .spend(*CREATOR, *PUBLISHER, 3.into())
            .expect("Should spend amount successfully");

        // 18 - DAI
        let actual_hash =
            get_state_root_hash(channel.id(), &balances, 18).expect("should get state root hash");

        assert_eq!(
            "15be990a567fe035369e3c308ca28d84a944a8acba484634249c0c4259d17162",
            hex::encode(actual_hash)
        );
    }

    #[test]
    /// Because the old test only had earners & it was using the old V4 Channel
    /// we re-use it in order to double check if we haven't change anything with the `get_state_root_hash()` changes
    /// when we introduced `spenders` `("spender", address, amount)` & `UnifiedNum`
    fn get_state_root_hash_returns_correct_hash_for_added_address_to_spenders() {
        let channel = "061d5e2a67d0a9a10f1c732bca12a676d83f79663a396f7d87b3e30b9b411088"
            .parse()
            .expect("Valid ChannelId");

        let mut balances = Balances::<CheckedState>::default();
        balances.add_earner(*PUBLISHER);

        // 18 - DAI
        let actual_hash =
            get_state_root_hash(channel, &balances, 18).expect("should get state root hash");

        assert_eq!(
            "7cca99ab2dfa751aaf53b8d52ca903f2095d06879f86a8c383294d15b797912c",
            hex::encode(actual_hash)
        );
    }
}
