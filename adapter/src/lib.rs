#![deny(rust_2018_idioms)]
#![deny(clippy::all)]
#![deny(clippy::match_bool)]

use std::error::Error;

use chrono::{DateTime, Utc};
use hex::FromHex;
use primitives::{channel::ChannelError, BigNum, Channel, ValidatorId};
use sha2::{Digest, Sha256};
use std::convert::TryFrom;
use tiny_keccak::Keccak;
use web3::{
    ethabi::{encode, token::Token},
    types::{Address, U256},
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

pub fn get_balance_leaf(acc: &ValidatorId, amnt: &BigNum) -> Result<[u8; 32], Box<dyn Error>> {
    let tokens = [
        Token::Address(Address::from_slice(acc.inner())),
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

// OnChain channel Representation
pub struct EthereumChannel {
    pub creator: Address,
    pub token_addr: Address,
    pub token_amount: U256,
    pub valid_until: U256,
    pub validators: Vec<Address>,
    pub spec: [u8; 32],
}

impl TryFrom<&Channel> for EthereumChannel {
    type Error = ChannelError;

    fn try_from(channel: &Channel) -> Result<Self, Self::Error> {
        let spec = serde_json::to_string(&channel.spec)
            .map_err(|e| ChannelError::InvalidArgument(e.to_string()))?;

        let mut hash = Sha256::new();
        hash.input(spec);
        let spec_hash: [u8; 32] = hash.result().into();

        let validators = channel
            .spec
            .validators
            .iter()
            .map(|v| &v.id)
            .collect::<Vec<_>>();

        let creator = channel.creator.inner();
        let deposit_asset = <[u8; 20]>::from_hex(&channel.deposit_asset[2..])
            .map_err(|_| ChannelError::InvalidArgument("failed to parse deposit asset".into()))?;

        EthereumChannel::new(
            &creator,
            &deposit_asset,
            &channel.deposit_amount.to_string(),
            channel.valid_until,
            &validators,
            &spec_hash,
        )
    }
}

impl EthereumChannel {
    pub fn new(
        creator: &[u8; 20],
        token_addr: &[u8; 20],
        token_amount: &str, // big num string
        valid_until: DateTime<Utc>,
        validators: &[&ValidatorId],
        spec: &[u8; 32],
    ) -> Result<Self, ChannelError> {
        if BigNum::try_from(token_amount).is_err() {
            return Err(ChannelError::InvalidArgument("invalid token amount".into()));
        }

        let creator = Address::from_slice(creator);
        let token_addr = Address::from_slice(token_addr);
        let token_amount = U256::from_dec_str(&token_amount)
            .map_err(|_| ChannelError::InvalidArgument("failed to parse token amount".into()))?;
        let valid_until = U256::from_dec_str(&valid_until.timestamp().to_string())
            .map_err(|_| ChannelError::InvalidArgument("failed to parse valid until".into()))?;

        let validators = validators
            .iter()
            .map(|v| Address::from_slice(v.inner()))
            .collect();

        Ok(Self {
            creator,
            token_addr,
            token_amount,
            valid_until,
            validators,
            spec: spec.to_owned(),
        })
    }

    pub fn hash(&self, contract_addr: &[u8; 20]) -> [u8; 32] {
        let tokens = [
            Token::Address(Address::from_slice(contract_addr)),
            Token::Address(self.creator.to_owned()),
            Token::Address(self.token_addr.to_owned()),
            Token::Uint(self.token_amount.to_owned()),
            Token::Uint(self.valid_until.to_owned()),
            Token::Array(
                self.validators
                    .iter()
                    .map(|v| Token::Address(v.to_owned()))
                    .collect(),
            ),
            Token::FixedBytes(self.spec.to_vec()),
        ];

        let encoded = encode(&tokens).to_vec();
        let mut result = Keccak::new_keccak256();
        result.update(&encoded);

        let mut res: [u8; 32] = [0; 32];
        result.finalize(&mut res);

        res
    }

    pub fn to_solidity_tuple(&self) -> Token {
        Token::Tuple(vec![
            Token::Address(self.creator.to_owned()),
            Token::Address(self.token_addr.to_owned()),
            Token::Uint(self.token_amount.to_owned()),
            Token::Uint(self.valid_until.to_owned()),
            Token::Array(
                self.validators
                    .iter()
                    .map(|v| Token::Address(v.to_owned()))
                    .collect(),
            ),
            Token::FixedBytes(self.spec.to_vec()),
        ])
    }

    pub fn hash_to_sign(
        &self,
        contract_addr: &str,
        balance_root: &str,
    ) -> Result<[u8; 32], Box<dyn Error>> {
        let root = <[u8; 32]>::from_hex(balance_root)?;
        let addr = hex::decode(contract_addr)?;
        get_signable_state_root(&addr, &root)
    }

    pub fn hash_to_sign_hex(
        &self,
        contract_addr: &str,
        balance_root: &str,
    ) -> Result<String, Box<dyn Error>> {
        let result = self.hash_to_sign(contract_addr, balance_root)?;
        Ok(format!("0x{}", hex::encode(result)))
    }
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
