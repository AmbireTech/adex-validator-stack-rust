#![deny(clippy::all)]
#![deny(rust_2018_idioms)]

use crate::EthereumChannel;
use base64;
use chrono::Utc;
use ethkey::{recover, sign, verify_address, Address, KeyPair, Message, Public, Signature};
use ethsign::{keyfile::KeyFile, Protected};
use primitives::adapter::{Adapter, AdapterError, AdapterOptions, AdapterResult, Session};
use primitives::channel_validator::ChannelValidator;
use primitives::config::Config;
use primitives::{Channel, ValidatorDesc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::TryFrom;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use web3::{
    contract::{Contract, Options},
    futures::Future,
    types::U256
};

use std::error::Error;

pub type Password = Protected;

#[derive(Debug, Clone)]
pub struct EthereumAdapter {
    keystore_json: String,
    keystore_pwd: String,
    ethereum_core_address: String,
    ethereum_network: String,
    tokens_verified: HashMap<String, Session>,
    tokens_for_auth: HashMap<String, String>,
    wallet: Option<KeyPair>,
}

// Enables EthereumAdapter to be able to
// check if a channel is valid
impl ChannelValidator for EthereumAdapter {}

impl Adapter for EthereumAdapter {
    type Output = EthereumAdapter;

    fn init(opts: AdapterOptions, config: &Config) -> EthereumAdapter {
        // @TODO ensure the keystore_json file exists
        // during program startup
        let keystore_json = opts.keystore_file.expect("Keystore file required");
        let keystore_pwd = opts.keystore_pwd.expect("Keystore password required");

        Self {
            keystore_json,
            keystore_pwd,
            tokens_verified: HashMap::new(),
            tokens_for_auth: HashMap::new(),
            wallet: None,
            ethereum_network: config.ethereum_network.clone(),
            ethereum_core_address: config.ethereum_core_address.clone(),
        }
    }

    fn unlock(&mut self) -> AdapterResult<bool> {
        let path = Path::new(&self.keystore_json).to_path_buf();
        let password: Password = self.keystore_pwd.clone().into();

        let json_file = File::open(&path).expect("Failed to load json file");
        let key_file: KeyFile = serde_json::from_reader(json_file).expect("Invalid keystore json");

        let plain_secret = key_file
            .crypto
            .decrypt(&password)
            .expect("Invalid keystore password");

        let keypair =
            KeyPair::from_secret_slice(&plain_secret.as_slice()).expect("Failed to create keypair");

        self.wallet = Some(keypair);

        // wallet has been unlocked
        Ok(true)
    }

    fn whoami(&self) -> String {
        match &self.wallet {
            Some(wallet) => format!("0x{}", wallet.address()),
            None => {
                eprintln!("Unlock wallet before use");
                "".to_string()
            }
        }
    }

    fn sign(&self, state_root: &str) -> AdapterResult<String> {
        let message = Message::from_slice(state_root.as_bytes());
        let wallet = self
            .wallet
            .clone()
            .expect("Unlock the wallet before signing");
        let signature = sign(wallet.secret(), &message).expect("sign message");

        Ok(format!("{}", signature))
    }

    fn verify(&self, signer: &str, state_root: &str, sig: &str) -> AdapterResult<bool> {
        let address = Address::from_slice(signer.as_bytes());
        let signature = Signature::from_str(sig).unwrap();
        let message = Message::from_slice(state_root.as_bytes());
        let result =
            verify_address(&address, &signature, &message).expect("Failed to verify signature");

        Ok(result)
    }

    fn validate_channel(&self, channel: &Channel) -> AdapterResult<bool> {
        let (_eloop, transport) = web3::transports::Http::new(&self.ethereum_network).unwrap();
        let web3 = web3::Web3::new(transport);
        let contract_address = Address::from_slice(self.ethereum_core_address.as_bytes());

        let contract = Contract::from_json(
            web3.eth(),
            contract_address,
            include_bytes!("../contract/AdExCore.json"),
        )
        .unwrap();

        let eth_channel: EthereumChannel = channel.into();

        let channel_id = eth_channel
            .hash_hex(&self.ethereum_core_address)
            .expect("Failed to hash the channel id");
        assert_eq!(channel_id, channel.id, "channel.id is not valid");

        // @TODO checksum ethereum address

        let is_channel_valid = ChannelValidator::is_channel_valid(channel);

        // query the blockchain for the channel status
        let contract_query = contract.query("states", channel_id, None, Options::default(), None);
        let channel_status: U256 = contract_query.wait().unwrap();

        assert_eq!(
            channel_status,
            1.into(),
            "channel is not Active on the ethereum network"
        );

        Ok(true)
    }

    fn session_from_token(&mut self, token: &str) -> AdapterResult<Session> {
        let token_id = token.to_owned()[..16].to_string();
        let mut result = self.tokens_verified.get(&token_id);
        if result.is_some() {
            return Ok(result.unwrap().to_owned());
        }

        let verified = match ewt_verify(&token) {
            Ok(v) => v,
            Err(e) => return Err(AdapterError::EwtVerifyFailed(format!("{}", e))),
        };

        // assert_eq!(self.wallet.unwrap().public(), verified.from, "token payload.id !== whoami(): token was not intended for us");
        let sess = match &verified.payload.identity {
            Some(identity) => {
                let (_eloop, transport) =
                    web3::transports::Http::new(&self.ethereum_network).unwrap();
                let web3 = web3::Web3::new(transport);

                let contract_address = Address::from_slice(self.ethereum_core_address.as_bytes());

                let contract = Contract::from_json(
                    web3.eth(),
                    contract_address,
                    include_bytes!("../contract/Identity.json"),
                )
                .unwrap();

                let contract_query = contract.query(
                    "privileges",
                    format!("{}", verified.from),
                    None,
                    Options::default(),
                    None,
                );
                let priviledge_level: U256 = contract_query.wait().unwrap();

                if priviledge_level == 0.into() {
                    return Err(AdapterError::Authorization(
                        "insufficient privilege".to_string(),
                    ));
                }
                Session {
                    era: verified.payload.era,
                    uid: identity.to_owned(),
                }
            }
            None => Session {
                era: verified.payload.era,
                uid: format!("{}", verified.from),
            },
        };

        self.tokens_verified.insert(token_id, sess.clone());
        Ok(sess)
    }

    fn get_auth(&mut self, validator: &ValidatorDesc) -> AdapterResult<String> {
        match self.tokens_for_auth.get(&validator.id) {
            Some(token) => Ok(token.to_owned()),
            None => {
                let payload = Payload {
                    id: validator.id.clone(),
                    era: usize::try_from(Utc::now().timestamp()).unwrap(),
                    identity: None,
                    address: None,
                };
                let token = ewt_sign(&self.wallet.clone().unwrap(), &payload)
                    .expect("Failed to sign token");
                self.tokens_for_auth
                    .insert(validator.id.clone(), token.clone());
                Ok(token)
            }
        }
    }
}

// Ethereum Web Tokens
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Payload {
    pub id: String,
    pub era: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<String>,
}

#[derive(Clone, Debug)]
pub struct VerifyPayload {
    pub from: Public,
    pub payload: Payload,
}

pub fn ewt_sign(signer: &KeyPair, payload: &Payload) -> Result<String, Box<dyn Error>> {
    let header_json = r#"
        {
            "type": "JWT",
            "alg": "ETH"
        }
    "#;
    let header = base64::encode(header_json);
    let payload_json = serde_json::to_string(&payload)?;
    let payload_encoded = base64::encode(&payload_json);
    let payload_string = format!("{}.{}", header, payload_encoded);

    let message = Message::from_slice(payload_string.as_bytes());
    let signature = sign(signer.secret(), &message)?;

    Ok(format!("{}.{}.{}", header, payload_encoded, signature))
}

pub fn ewt_verify(token: &str) -> Result<VerifyPayload, Box<dyn Error>> {
    let parts: Vec<String> = token.split(".").map(ToString::to_string).collect();
    assert_eq!(parts.len(), 3, "verify: token needs to be of 3 parts");

    let part1 = format!("{}", parts.get(1).unwrap());

    let msg = format!("{}.{}", parts.get(0).unwrap(), part1);
    let message = Message::from_slice(msg.as_bytes());

    let sig = base64::decode(parts.get(2).unwrap())?;
    let signature = Signature::from_str(&hex::encode(&sig.as_slice())).unwrap();

    let public_key = recover(&signature, &message)?;

    let decode_part1 = base64::decode(&part1)?;
    let payload_string = String::from_utf8(decode_part1)?;

    let payload: Payload = serde_json::from_str(&payload_string)?;

    let verified_payload = VerifyPayload {
        from: public_key,
        payload,
    };

    Ok(verified_payload)
}
