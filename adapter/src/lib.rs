#![deny(rust_2018_idioms)]
#![deny(clippy::all)]
#![deny(clippy::match_bool)]

use std::error::Error;

use primitives::{channel::ChannelError, Address, BigNum};
use tiny_keccak::Keccak;
use web3::{
    ethabi::{encode, token::Token},
    types::{Address as EthAddress, U256},
};

pub use self::dummy::DummyAdapter;
pub use self::ethereum::EthereumAdapter;

pub mod dummy;
pub mod ethereum;

pub enum AdapterTypes {
    DummyAdapter(Box<DummyAdapter>),
    EthereumAdapter(Box<EthereumAdapter>),
}

pub fn get_signable_state_root(
    channel_id: &[u8],
    balance_root: &[u8; 32],
) -> Result<[u8; 32], Box<dyn Error>> {
    let tokens = [
        Token::FixedBytes(channel_id.to_vec()),
        Token::FixedBytes(balance_root.to_vec()),
    ];

    let encoded = encode(&tokens).to_vec();

    let mut result = Keccak::new_keccak256();
    result.update(&encoded);

    let mut res: [u8; 32] = [0; 32];
    result.finalize(&mut res);

    Ok(res)
}

pub fn get_balance_leaf(acc: &Address, amnt: &BigNum) -> Result<[u8; 32], Box<dyn Error>> {
    let tokens = [
        Token::Address(EthAddress::from_slice(acc.as_bytes())),
        Token::Uint(
            U256::from_dec_str(&amnt.to_str_radix(10))
                .map_err(|_| ChannelError::InvalidArgument("failed to parse amt".into()))?,
        ),
    ];
    let encoded = encode(&tokens).to_vec();

    let mut result = Keccak::new_keccak256();
    result.update(&encoded);

    let mut res: [u8; 32] = [0; 32];
    result.finalize(&mut res);

    Ok(res)
}

#[cfg(test)]
mod test {
    use std::convert::TryFrom;

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
        )
        .expect("Should get state_root");

        let expected_hex =
            hex::decode("b68cde9b0c8b63ac7152e78a65c736989b4b99bfc252758b1c3fd6ca357e0d6b")
                .expect("Should decode valid expected hex");

        assert_eq!(state_root.to_vec(), expected_hex);
    }
}
