#![feature(async_await, await_macro)]
#![deny(rust_2018_idioms)]
#![deny(clippy::all)]
#![deny(clippy::match_bool)]
#![doc(test(attr(feature(async_await, await_macro))))]

use ethabi::encode;
use ethabi::param_type::{ParamType, Reader};
use ethabi::token::{LenientTokenizer, StrictTokenizer, Token, Tokenizer};
use std::error::Error;
use tiny_keccak::Keccak;
use primitives::Channel;
use sha2::{Sha256, Digest};
use primitives::BigNum;
use std::convert::From;
use hex;

pub mod dummy;
pub mod ethereum;

pub use self::dummy::DummyAdapter;
pub use self::ethereum::EthereumAdapter;

pub enum AdapterTypes {
    DummyAdapter(DummyAdapter),
    EthereumAdapter(EthereumAdapter),
}

pub fn get_signable_state_root(
    channel_id: &str,
    balance_root: &str,
) -> Result<[u8; 32], Box<dyn Error>> {
    let types = ["bytes32".to_string(), "bytes32".to_string()];
    let values = [channel_id.to_string(), balance_root.to_string()];
    let encoded = encode_params(&types, &values, true)?;

    let mut result = Keccak::new_keccak256();
    result.update(&encoded);

    let mut res: [u8; 32] = [0; 32];
    result.finalize(&mut res);

    Ok(res)
}

pub fn get_balance_leaf(acc: &str, amnt: &str) -> Result<[u8; 32], Box<dyn Error>> {
    let types: Vec<String> = vec!["address".to_string(), "uint256".to_string()];
    let values = [acc.to_string(), amnt.to_string()];
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
    pub valid_until: i64,
    pub validators: Vec<String>,
    pub spec: String,
}

impl From<&Channel> for EthereumChannel {
    fn from(channel: &Channel) -> Self {
        // let spec = 
        let spec = serde_json::to_string(&channel.spec).expect("Failed to serialize channel spec");

        let mut hash = Sha256::new();
        hash.input(spec);

        let spec_hash = format!("{:02x}", hash.result());

        let validators: Vec<String> = channel.spec.validators.into_iter().map(|v| v.id.clone()).collect();

        EthereumChannel::new(
            &channel.creator, 
            &channel.deposit_asset, 
            &channel.deposit_amount.to_string(), 
            channel.valid_until.timestamp(), 
            validators, 
            &spec_hash
        )
    }
}

impl EthereumChannel {
    pub fn new(
        creator: &str,
        token_addr: &str,
        token_amount: &str,
        valid_until: i64,
        validators: Vec<String>,
        spec: &str,
    ) -> Self {
        //@TODO some validation
        Self {
            creator: creator.to_owned(),
            token_addr: token_addr.to_owned(),
            token_amount: token_amount.to_owned(),
            valid_until,
            validators,
            spec: spec.to_owned(),
        }
    }

    pub fn hash(&self, contract_addr: &str) -> Result<[u8; 32], Box<dyn Error>> {
        let types: Vec<String> = vec![
            "address",
            "address",
            "address",
            "uint256",
            "uint256",
            "address[]",
            "bytes32",
        ]
        .into_iter()
        .map(ToString::to_string)
        .collect();

        let validators = format!("[ {} ]", self.validators.join(", "));

        let values = [
            contract_addr.to_string(),
            self.creator.to_owned(),
            self.token_addr.to_owned(),
            self.token_amount.to_owned(),
            self.valid_until.to_string(),
            validators,
            self.spec.to_owned()
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
        let validators = format!("[ {} ]", self.validators.join(", "));
        let spec = hex::encode(&self.spec);

        vec![
            self.creator.to_owned(),
            self.token_addr.to_owned(),
            format!("0x{}", self.token_amount.to_owned()),
            format!("0x{}", self.valid_until.to_owned()),
            validators,
            spec,
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
    types: &[String],
    values: &[String],
    lenient: bool,
) -> Result<Vec<u8>, Box<dyn Error>> {
    assert_eq!(types.len(), values.len());

    let types: Vec<ParamType> = types
        .iter()
        .map(|s| Reader::read(s))
        .collect::<Result<_, _>>()?;

    let params: Vec<_> = types
        .into_iter()
        .zip(values.iter().map(|s| s as &str))
        .collect();

    let tokens = parse_tokens(&params, lenient)?;
    let result = encode(&tokens);

    Ok(result.to_vec())
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
    use super::*;
    use byteorder::{BigEndian, ByteOrder};
    use chrono::{TimeZone, Utc};
    use primitives::merkle_tree::MerkleTree;
    use std::convert::TryFrom;

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
