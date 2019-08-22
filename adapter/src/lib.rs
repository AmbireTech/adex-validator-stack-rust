#![feature(async_await, await_macro)]
#![deny(rust_2018_idioms)]
#![deny(clippy::all)]
#![deny(clippy::match_bool)] 
#![doc(test(attr(feature(async_await, await_macro))))]
#![doc(test(attr(cfg(feature = "dummy-adapter"))))]
//pub use self::adapter::*;
//pub use self::sanity::*;
//
//mod adapter;
//#[cfg(any(test, feature = "dummy-adapter"))]

use primitives::big_num::BigNum;
use chrono::{DateTime, Utc};
use ethabi::param_type::{ParamType, Reader};
use ethabi::token::{Token, Tokenizer, StrictTokenizer, LenientTokenizer};
use ethabi::{encode};
use tiny_keccak::Keccak;
use std::error::Error;
use hex::{ToHex, FromHex};

pub mod dummy;
pub mod ethereum;

pub use self::dummy::DummyAdapter;
pub use self::ethereum::EthereumAdapter;


pub fn get_signable_state_root( channel_id: &str, balance_root: &str ) -> Result<[u8; 32], Box<dyn Error>> {
        let types: Vec<String> = vec![
                "bytes32".to_string(), 
                "bytes32".to_string(), 
        ];
        let values = [channel_id.to_string(), balance_root.to_string()];
        let encoded = encode_params(&types, &values, true)?;

        let mut result = Keccak::new_sha3_256();
        result.update(encoded.as_ref());
        
        let mut res: [u8; 32] = [0; 32];
        result.finalize(&mut res);

        Ok(res)

}

pub fn get_balance_leaf(acc: &str, amnt: &str) -> Result<[u8; 32], Box<dyn Error>> {
        let types: Vec<String> = vec![
                "address".to_string(), 
                "uint256".to_string(), 
        ];
        let values = [acc.to_string(), amnt.to_string()];
        let encoded = encode_params(&types, &values, true)?;

        let mut result = Keccak::new_sha3_256();
        result.update(encoded.as_ref());
        
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
    pub spec: String
}

impl EthereumChannel {

    fn new(creator: &str, token_addr: &str, token_amount: String, valid_until: String, validators: String, spec: String) -> Self {
        //@TODO some validation
        Self {
            creator: creator.to_owned(),
            token_addr: token_addr.to_owned(),
            token_amount: token_amount.to_owned(),
            valid_until,
            validators,
            spec
        }
    }

    fn hash(&self, contract_addr: &str) ->  Result<[u8; 32], Box<dyn Error>>  {
        let types: Vec<String> = vec![
                "address", 
                "address", 
                "address", 
                "uint256", 
                "uint256", 
                "address[]", 
                "bytes32"].into_iter()
                          .map(ToString::to_string)
                          .collect();

        let values = [
                contract_addr.to_string(),
                self.creator.to_owned(),
                self.token_addr.to_owned(),
                self.token_amount.to_owned(),
                self.valid_until.to_owned(),
                self.validators.to_owned(),
                self.spec.to_owned()

        ];
        let encoded = encode_params(&types, &values, true)?;
        let mut result = Keccak::new_sha3_256();
        result.update(encoded.as_ref());
        
        let mut res: [u8; 32] = [0; 32];
        result.finalize(&mut res);

        Ok(res)
    }

    fn hash_hex(&self, contract_addr: &str) -> Result<String,Box<dyn Error>>  {
        let result = self.hash(contract_addr)?;
        Ok(format!("0x{}", hex::encode(result).to_string()))
    }

    fn to_solidity_tuple(&self) -> Vec<String> {
        vec![
                self.creator.to_owned(),
                self.token_addr.to_owned(),
                format!("0x{}", self.token_amount.to_owned()),
                format!("0x{}", self.valid_until.to_owned()),
                self.validators.to_owned(),
                self.spec.to_owned()
        ]
    }

    fn hash_to_sign(&self, contract_addr: &str, balance_root: &str) -> Result<[u8; 32], Box<dyn Error>> {
        get_signable_state_root(contract_addr, balance_root)
    }

    fn hash_to_sign_hex(&self, contract_addr: &str, balance_root: &str) ->  Result<String, Box<dyn Error>> {
        let result = self.hash_to_sign(contract_addr, balance_root)?;
        Ok(format!("0x{}", hex::encode(result).to_string()))
    }

}

fn encode_params(types: &[String], values: &[String], lenient: bool) -> Result<String, Box<dyn Error>> {
	assert_eq!(types.len(), values.len());

	let types: Vec<ParamType> = types.iter()
		.map(|s| Reader::read(s))
		.collect::<Result<_, _>>()?;

	let params: Vec<_> = types.into_iter()
		.zip(values.iter().map(|v| v as &str))
		.collect();

	let tokens = parse_tokens( &params, lenient)?;
	let result = encode(&tokens);

	Ok(hex::encode(result).to_string())
}

fn parse_tokens(params: &[(ParamType, &str)], lenient: bool) -> Result<Vec<Token>, Box< dyn Error>> {
	params.iter()
		.map(|&(ref param, value)| if lenient {
			 LenientTokenizer::tokenize(param, value)
        } else {
			StrictTokenizer::tokenize(param, value)
		})
		.collect::<Result<_, _>>()
		.map_err(From::from)
}

fn to_ethereum_channel() -> EthereumChannel {
        unimplemented!()
}