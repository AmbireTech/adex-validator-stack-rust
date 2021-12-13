pub use {
    adapter::state::{LockedState, UnlockedState},
    error::Error,
};

/// only re-export types from the `primitives` crate that are being used by the [`crate::Adapter`].
pub mod primitives {
    use serde::{Deserialize, Serialize};
    use thiserror::Error;

    /// Re-export all the types used from the [`primitives`] crate.
    pub use ::primitives::{Address, BigNum, Channel, ValidatorId};
    use web3::{
        ethabi::{encode, Address as EthAddress, Token},
        signing::keccak256,
        types::U256,
    };

    pub type Deposit = ::primitives::Deposit<primitives::BigNum>;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Session {
        pub era: i64,
        pub uid: Address,
    }

    #[derive(Debug, Error)]
    #[error("{0}")]
    pub struct BalanceLeafError(String);

    pub fn get_signable_state_root(channel_id: &[u8], balance_root: &[u8; 32]) -> [u8; 32] {
        let tokens = [
            Token::FixedBytes(channel_id.to_vec()),
            Token::FixedBytes(balance_root.to_vec()),
        ];

        let encoded = encode(&tokens).to_vec();

        keccak256(&encoded)
    }

    pub fn get_balance_leaf(
        is_spender: bool,
        acc: &Address,
        amnt: &BigNum,
    ) -> Result<[u8; 32], BalanceLeafError> {
        let address = Token::Address(EthAddress::from_slice(acc.as_bytes()));
        let amount = Token::Uint(
            U256::from_dec_str(&amnt.to_str_radix(10))
                .map_err(|_| BalanceLeafError("Failed to parse amt".into()))?,
        );

        let tokens = if is_spender {
            vec![Token::String("spender".into()), address, amount]
        } else {
            vec![address, amount]
        };
        let encoded = encode(&tokens).to_vec();

        Ok(keccak256(&encoded))
    }

    #[cfg(test)]
    mod test {
        use byteorder::{BigEndian, ByteOrder};
        use chrono::{TimeZone, Utc};

        use primitives::merkle_tree::MerkleTree;

        use super::*;

        #[test]
        fn get_signable_state_root_hash_is_aligned_with_js_impl() {
            let timestamp = Utc.ymd(2019, 9, 12).and_hms(17, 0, 0);
            let mut timestamp_buf = [0_u8; 32];
            let n: u64 = u64::try_from(timestamp.timestamp_millis())
                .expect("The timestamp should be able to be converted to u64");
            BigEndian::write_uint(&mut timestamp_buf[26..], n, 6);

            let merkle_tree = MerkleTree::new(&[timestamp_buf]).expect("Should instantiate");

            let channel_id = "061d5e2a67d0a9a10f1c732bca12a676d83f79663a396f7d87b3e30b9b411088";

            let state_root = get_signable_state_root(
                &hex::decode(&channel_id).expect("failed"),
                &merkle_tree.root(),
            );

            let expected_hex =
                hex::decode("b68cde9b0c8b63ac7152e78a65c736989b4b99bfc252758b1c3fd6ca357e0d6b")
                    .expect("Should decode valid expected hex");

            assert_eq!(state_root.to_vec(), expected_hex);
        }
    }
}

/// Re-export of the [`crate::client`] traits.
pub mod prelude {
    /// Re-export traits used for working with the [`crate::Adapter`].
    pub use crate::client::{Locked, Unlockable, Unlocked};

    pub use crate::{LockedState, UnlockedState};
}

/// The [`Adapter`] struct and it's states
/// Used for communication with underlying implementation.
/// 2 Adapters are available in this crate:
/// - [`crate::ethereum::Adapter`] and it's client implementation [`crate::Ethereum`] for chains compatible with EVM.
/// - [`crate::dummy::Adapter`] and it's client implementation [`crate::Dummy`] for testing.
mod adapter;
pub mod client;
mod error;

pub use adapter::Adapter;

/// Dummy testing client for [`Adapter`]
pub use dummy::Dummy;
/// Ethereum Client
pub use ethereum::Ethereum;

pub mod dummy;
pub mod ethereum;
