#![deny(rust_2018_idioms)]
#![deny(clippy::all)]

use adapter::{get_balance_leaf, get_signable_state_root, BalanceLeafError};
use primitives::{
    balances::CheckedState,
    merkle_tree::{Error as MerkleTreeError, MerkleTree},
    Balances, ChannelId,
};
use thiserror::Error;

pub use self::sentry_interface::{all_channels, SentryApi};

pub mod channel;
pub mod error;
pub mod follower;
pub mod heartbeat;
pub mod leader;
pub mod sentry_interface;

pub mod core {
    pub mod follower_rules;
}

#[derive(Debug, Error)]
pub enum StateRootHashError {
    #[error("Failed to get balance leaf")]
    BalanceLeaf(#[from] BalanceLeafError),
    #[error(transparent)]
    MerkleTree(#[from] MerkleTreeError),
}

pub(crate) fn get_state_root_hash(
    channel: ChannelId,
    balances: &Balances<CheckedState>,
    token_precision: u8,
) -> Result<[u8; 32], StateRootHashError> {
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

    use primitives::util::tests::prep_db::{ADDRESSES, DUMMY_CAMPAIGN, DUMMY_CHANNEL};

    #[test]
    // TODO: Double check this test - encoded value! after introducing `spenders` ("spender", address, amount)
    fn get_state_root_hash_returns_correct_hash() {
        let channel = DUMMY_CAMPAIGN.channel;

        let mut balances = Balances::<CheckedState>::default();

        balances
            .spend(ADDRESSES["tester"], ADDRESSES["publisher"], 3.into())
            .expect("Should spend amount successfully");

        // 18 - DAI
        let actual_hash =
            get_state_root_hash(channel.id(), &balances, 18).expect("should get state root hash");

        assert_eq!(
            "b80c1ac35ca5a6cf99996bfaaf0a9b1b446ed6f0157c9102e7b2d035519ae03d",
            hex::encode(actual_hash)
        );
    }

    #[test]
    /// Because the old test only had earners & it was using the old V4 Channel
    /// we re-use it in order to double check if we haven't change anything with the `get_state_root_hash()` changes
    /// when we introduced `spenders` `("spender", address, amount)` & `UnifiedNum`
    fn get_state_root_hash_returns_correct_hash_for_added_address_to_spenders() {
        let channel = DUMMY_CHANNEL.id;

        let mut balances = Balances::<CheckedState>::default();
        balances.add_earner(ADDRESSES["publisher"]);

        // 18 - DAI
        let actual_hash =
            get_state_root_hash(channel, &balances, 18).expect("should get state root hash");

        assert_eq!(
            "4fad5375c3ef5f8a9d23a8276fed0151164dea72a5891cec8b43e1d190ed430e",
            hex::encode(actual_hash)
        );
    }
}
