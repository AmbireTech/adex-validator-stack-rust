#![deny(rust_2018_idioms)]
#![deny(clippy::all)]
#![deny(clippy::match_bool)]

use std::error::Error;

use chrono::{DateTime, Utc};
use ethabi::encode;
use ethabi::param_type::ParamType;
use ethabi::token::{LenientTokenizer, StrictTokenizer, Tokenizer};
use primitives::channel::ChannelError;
use primitives::BigNum;
use primitives::Channel;
use sha2::{Digest, Sha256};
use std::convert::TryFrom;
use tiny_keccak::Keccak;

pub use self::dummy::DummyAdapter;
pub use self::ethereum::EthereumAdapter;

pub mod dummy;
pub mod ethereum;

pub enum AdapterTypes {
    DummyAdapter(Box<DummyAdapter>),
    EthereumAdapter(Box<EthereumAdapter>),
}

pub fn get_signable_state_root(
    channel_id: &str,
    balance_root: &str,
) -> Result<[u8; 32], Box<dyn Error>> {
    let params = [
        (ParamType::FixedBytes(32), channel_id),
        (ParamType::FixedBytes(32), balance_root),
    ];
    let encoded = encode_params(&params, true)?;

    let mut result = Keccak::new_keccak256();
    result.update(&encoded);

    let mut res: [u8; 32] = [0; 32];
    result.finalize(&mut res);

    Ok(res)
}

pub fn get_balance_leaf(acc: &str, amnt: &BigNum) -> Result<[u8; 32], Box<dyn Error>> {
    let params = [
        (ParamType::Address, acc),
        (ParamType::Uint(256), &amnt.to_str_radix(16)),
    ];
    let encoded = encode_params(&params, true)?;

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
    pub validators: String,
    pub spec: String,
}

impl TryFrom<&Channel> for EthereumChannel {
    type Error = ChannelError;

    fn try_from(channel: &Channel) -> Result<Self, Self::Error> {
        let spec = serde_json::to_string(&channel.spec)
            .map_err(|e| ChannelError::InvalidArgument(e.to_string()))?;

        let mut hash = Sha256::new();
        hash.input(spec);

        let spec_hash = format!("{:02x}", hash.result());

        let validators = channel
            .spec
            .validators
            .into_iter()
            .map(|v| v.id.clone())
            .collect();

        EthereumChannel::new(
            &channel.creator,
            &channel.deposit_asset,
            &channel.deposit_amount.to_string(),
            channel.valid_until,
            validators,
            &spec_hash,
        )
    }
}

impl EthereumChannel {
    pub fn new(
        creator: &str,
        token_addr: &str,
        token_amount: &str,
        valid_until: DateTime<Utc>,
        validators: Vec<String>,
        spec: &str,
    ) -> Result<Self, ChannelError> {
        // check creator addres
        if creator != eth_checksum::checksum(creator) {
            return Err(ChannelError::InvalidArgument(
                "Invalid creator address".into(),
            ));
        }

        if token_addr != eth_checksum::checksum(token_addr) {
            return Err(ChannelError::InvalidArgument(
                "invalid token addresss".into(),
            ));
        }

        if BigNum::try_from(token_amount).is_err() {
            return Err(ChannelError::InvalidArgument("invalid token amount".into()));
        }

        if spec.len() != 32 {
            return Err(ChannelError::InvalidArgument(
                "32 len string expected".into(),
            ));
        }

        if validators.iter().any(|v| *v != eth_checksum::checksum(v)) {
            return Err(ChannelError::InvalidArgument(
                "invalid validator address: must start with a 0x and be 42 characters long".into(),
            ));
        }

        Ok(Self {
            creator: creator.to_owned(),
            token_addr: token_addr.to_owned(),
            token_amount: token_amount.to_owned(),
            valid_until: valid_until.timestamp(),
            validators: format!("[{}]", validators.join(",")),
            spec: spec.to_owned(),
        })
    }

    pub fn hash(&self, contract_addr: &str) -> Result<[u8; 32], Box<dyn Error>> {
        let params = [
            (ParamType::Address, contract_addr),
            (ParamType::Address, &self.creator),
            (ParamType::Address, &self.token_addr),
            (ParamType::Uint(256), &self.token_amount),
            (ParamType::Uint(256), &self.valid_until.to_string()),
            (
                ParamType::Array(Box::new(ParamType::Address)),
                &self.validators,
            ),
            (ParamType::FixedBytes(32), &self.spec),
        ];

        let encoded = encode_params(&params, true)?;
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

fn encode_params(params: &[(ParamType, &str)], lenient: bool) -> Result<Vec<u8>, Box<dyn Error>> {
    let tokens = params
        .iter()
        .map(|(param, value)| {
            if lenient {
                LenientTokenizer::tokenize(param, value)
            } else {
                StrictTokenizer::tokenize(param, value)
            }
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(encode(&tokens).to_vec())
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
