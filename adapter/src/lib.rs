#![feature(async_await, await_macro)]
#![deny(rust_2018_idioms)]
#![deny(clippy::all)]
#![deny(clippy::match_bool)]
#![doc(test(attr(feature(async_await, await_macro))))]

use std::error::Error;

use ethabi::encode;
use ethabi::param_type::{ParamType, Reader};
use ethabi::token::{LenientTokenizer, StrictTokenizer, Token, Tokenizer};
use tiny_keccak::Keccak;

use primitives::BigNum;

pub use self::dummy::DummyAdapter;
pub use self::ethereum::EthereumAdapter;

pub mod dummy;
pub mod ethereum;

pub enum AdapterTypes {
    DummyAdapter(DummyAdapter),
    EthereumAdapter(EthereumAdapter),
}

pub fn get_signable_state_root(
    channel_id: &str,
    balance_root: &str,
) -> Result<[u8; 32], Box<dyn Error>> {
    let types = ["bytes32", "bytes32"];
    let values = [channel_id, balance_root];
    let encoded = encode_params(&types, &values, true)?;

    let mut result = Keccak::new_keccak256();
    result.update(&encoded);

    let mut res: [u8; 32] = [0; 32];
    result.finalize(&mut res);

    Ok(res)
}

pub fn get_balance_leaf(acc: &str, amnt: &BigNum) -> Result<[u8; 32], Box<dyn Error>> {
    let types = ["address", "uint256"];
    let values = [acc, &amnt.to_str_radix(16)];
    let encoded = encode_params(&types, &values, true)?;

    let mut result = Keccak::new_keccak256();
    result.update(&encoded);

    let mut res: [u8; 32] = [0; 32];
    result.finalize(&mut res);

    Ok(res)
}

// OnChain channel Representation
pub struct EthereumChannel {
    pub creator: String,
    pub token_addr: String,
    pub token_amount: String,
    pub valid_until: String,
    pub validators: String,
    pub spec: String,
}

impl EthereumChannel {
    pub fn new(
        creator: &str,
        token_addr: &str,
        token_amount: String,
        valid_until: String,
        validators: String,
        spec: String,
    ) -> Self {
        //@TODO some validation
        Self {
            creator: creator.to_owned(),
            token_addr: token_addr.to_owned(),
            token_amount: token_amount.to_owned(),
            valid_until,
            validators,
            spec,
        }
    }

    pub fn hash(&self, contract_addr: &str) -> Result<[u8; 32], Box<dyn Error>> {
        let types = [
            "address",
            "address",
            "address",
            "uint256",
            "uint256",
            "address[]",
            "bytes32",
        ];

        let values = [
            contract_addr,
            &self.creator,
            &self.token_addr,
            &self.token_amount,
            &self.valid_until,
            &self.validators,
            &self.spec,
        ];
        let encoded = encode_params(&types, &values, true)?;
        let mut result = Keccak::new_keccak256();
        result.update(&encoded);

        let mut res: [u8; 32] = [0; 32];
        result.finalize(&mut res);

        Ok(res)
    }

    pub fn hash_hex(&self, contract_addr: &str) -> Result<String, Box<dyn Error>> {
        let result = self.hash(contract_addr)?;
        Ok(format!("0x{}", hex::encode(result).to_string()))
    }

    pub fn to_solidity_tuple(&self) -> Vec<String> {
        vec![
            self.creator.to_owned(),
            self.token_addr.to_owned(),
            format!("0x{}", self.token_amount.to_owned()),
            format!("0x{}", self.valid_until.to_owned()),
            self.validators.to_owned(),
            self.spec.to_owned(),
        ]
    }

    pub fn hash_to_sign(
        &self,
        contract_addr: &str,
        balance_root: &str,
    ) -> Result<[u8; 32], Box<dyn Error>> {
        get_signable_state_root(contract_addr, balance_root)
    }

    pub fn hash_to_sign_hex(
        &self,
        contract_addr: &str,
        balance_root: &str,
    ) -> Result<String, Box<dyn Error>> {
        let result = self.hash_to_sign(contract_addr, balance_root)?;
        Ok(format!("0x{}", hex::encode(result).to_string()))
    }
}

fn encode_params(
    types: &[&str],
    values: &[&str],
    lenient: bool,
) -> Result<Vec<u8>, Box<dyn Error>> {
    assert_eq!(types.len(), values.len());

    let types: Vec<ParamType> = types
        .iter()
        .map(|s| Reader::read(s))
        .collect::<Result<_, _>>()?;

    let params = types.into_iter().zip(values.to_vec()).collect::<Vec<_>>();

    let tokens = parse_tokens(&params, lenient)?;

    Ok(encode(&tokens).to_vec())
}

fn parse_tokens(params: &[(ParamType, &str)], lenient: bool) -> Result<Vec<Token>, Box<dyn Error>> {
    params
        .iter()
        .map(|&(ref param, value)| {
            if lenient {
                LenientTokenizer::tokenize(param, value)
            } else {
                StrictTokenizer::tokenize(param, value)
            }
        })
        .collect::<Result<_, _>>()
        .map_err(From::from)
}

#[cfg(test)]
mod test {
    use std::convert::TryFrom;

    use byteorder::{BigEndian, ByteOrder};
    use chrono::{TimeZone, Utc};

    use primitives::merkle_tree::MerkleTree;

    use super::*;

    #[test]
    fn test_get_signable_state_root_hash_is_aligned_with_js_impl() {
        let timestamp = Utc.ymd(2019, 9, 12).and_hms(17, 0, 0);
        let mut timestamp_buf = [0_u8; 32];
        let n: u64 = u64::try_from(timestamp.timestamp_millis())
            .expect("The timestamp should be able to be converted to u64");
        BigEndian::write_uint(&mut timestamp_buf[26..], n, 6);

        let merkle_tree = MerkleTree::new(&[timestamp_buf]);
        let info_root_raw = hex::encode(merkle_tree.root());

        let channel_id = "061d5e2a67d0a9a10f1c732bca12a676d83f79663a396f7d87b3e30b9b411088";

        let state_root =
            get_signable_state_root(&channel_id, &info_root_raw).expect("Should get state_root");

        let expected_hex =
            hex::decode("b68cde9b0c8b63ac7152e78a65c736989b4b99bfc252758b1c3fd6ca357e0d6b")
                .expect("Should decode valid expected hex");

        assert_eq!(state_root.to_vec(), expected_hex);
    }
}
