#![deny(rust_2018_idioms)]
#![deny(clippy::all)]

use std::error::Error;

use adapter::{get_balance_leaf, get_signable_state_root};
use primitives::adapter::Adapter;
use primitives::merkle_tree::MerkleTree;
use primitives::BalancesMap;

pub use self::sentry_interface::{all_channels, SentryApi};

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

    let tree = MerkleTree::new(&elems)?;
    // keccak256(channelId, balanceRoot
    get_signable_state_root(iface.channel.id.as_ref(), &tree.root())
}

#[cfg(test)]
mod test {
    use super::*;

    use adapter::DummyAdapter;
    use primitives::adapter::DummyAdapterOptions;
    use primitives::config::configuration;
    use primitives::util::tests::prep_db::{AUTH, DUMMY_CHANNEL, IDS};
    use primitives::{BalancesMap, Channel};
    use slog::{o, Discard, Logger};

    fn setup_iface(channel: &Channel) -> SentryApi<DummyAdapter> {
        let adapter_options = DummyAdapterOptions {
            dummy_identity: IDS["leader"].clone(),
            dummy_auth: IDS.clone(),
            dummy_auth_tokens: AUTH.clone(),
        };
        let config = configuration("development", None).expect("Dev config should be available");
        let dummy_adapter = DummyAdapter::init(adapter_options, &config);
        let logger = Logger::root(Discard, o!());

        SentryApi::init(dummy_adapter, channel.clone(), &config, logger).expect("should succeed")
    }

    #[test]
    fn get_state_root_hash_returns_correct_hash_aligning_with_js_impl() {
        let channel = DUMMY_CHANNEL.clone();

        let iface = setup_iface(&channel);

        let balances: BalancesMap = vec![
            (IDS["publisher"].clone(), 1.into()),
            (IDS["tester"].clone(), 2.into()),
        ]
        .into_iter()
        .collect();

        let actual_hash =
            get_state_root_hash(&iface, &balances).expect("should get state root hash");

        assert_eq!(
            "d6c784be61c4d2c47a52cc72af6c133d24b163ad053ac7f0a65091001f43dda1",
            hex::encode(actual_hash)
        );
    }

    #[test]
    fn get_state_root_hash_returns_correct_hash_for_fake_channel_aligning_with_js_impl() {
        let channel = DUMMY_CHANNEL.clone();

        let iface = setup_iface(&channel);

        let balances: BalancesMap = vec![(IDS["publisher"].clone(), 0.into())]
            .into_iter()
            .collect();

        let actual_hash =
            get_state_root_hash(&iface, &balances).expect("should get state root hash");

        assert_eq!(
            "4fad5375c3ef5f8a9d23a8276fed0151164dea72a5891cec8b43e1d190ed430e",
            hex::encode(actual_hash)
        );
    }
}
