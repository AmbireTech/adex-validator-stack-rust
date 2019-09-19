#![deny(clippy::all)]
#![deny(rust_2018_idioms)]

use crate::EthereumChannel;
use base64;
use chrono::Utc;
use ethkey::{recover, sign, verify_address, Address, KeyPair, Message, Public, Signature};
use ethsign::{keyfile::KeyFile, Protected};
use primitives::{
    adapter::{Adapter, AdapterError, AdapterOptions, AdapterResult, Session},
    channel_validator::ChannelValidator,
    config::Config,
    Channel, ValidatorDesc,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::TryFrom;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use web3::{
    contract::{Contract, Options},
    futures::Future,
    types::U256,
};
use tiny_keccak::Keccak;

use std::error::Error;

pub type Password = Protected;

#[derive(Debug, Clone)]
pub struct EthereumAdapter {
    keystore_json: String,
    keystore_pwd: String,
    config: Config,
    tokens_verified: HashMap<String, Session>,
    tokens_for_auth: HashMap<String, String>,
    wallet: Option<KeyPair>,
}

// Enables EthereumAdapter to be able to
// check if a channel is valid
impl ChannelValidator for EthereumAdapter {}

impl Adapter for EthereumAdapter {
    type Output = EthereumAdapter;

    fn init(opts: AdapterOptions, config: &Config) -> AdapterResult<EthereumAdapter> {
        let keystore_json = match opts.keystore_file {
            Some(file) => file,
            None => {
                return Err(AdapterError::Configuration(
                    "Missing keystore json file".to_string(),
                ))
            }
        };

        let keystore_pwd = match opts.keystore_pwd {
            Some(file) => file,
            None => {
                return Err(AdapterError::Configuration(
                    "Missing keystore pwd".to_string(),
                ))
            }
        };

        Ok(Self {
            keystore_json,
            keystore_pwd,
            tokens_verified: HashMap::new(),
            tokens_for_auth: HashMap::new(),
            wallet: None,
            config: config.to_owned(),
        })
    }

    fn unlock(&mut self) -> AdapterResult<bool> {
        let path = Path::new(&self.keystore_json).to_path_buf();
        let password: Password = self.keystore_pwd.clone().into();

        let json_file = match File::open(&path) {
            Ok(data) => data,
            Err(e) => {
                return Err(AdapterError::Configuration(
                    "Invalid keystore location provided".to_string(),
                ))
            }
        };
        println!("{:?}", json_file);
        let key_file: KeyFile = match serde_json::from_reader(json_file) {
            Ok(data) => data,
            Err(e) => {
                return Err(AdapterError::Configuration(
                    format!("{}", e)
                ))
            }
        };

        let plain_secret = match key_file.crypto.decrypt(&password) {
            Ok(data) => data,
            Err(e) => {
                return Err(AdapterError::Configuration(
                    "Invalid keystore password provided".to_string(),
                ))
            }
        };

        let keypair =
            KeyPair::from_secret_slice(&plain_secret.as_slice()).expect("Failed to create keypair");

        self.wallet = Some(keypair);

        // wallet has been unlocked
        Ok(true)
    }

    fn whoami(&self) -> AdapterResult<String> {
        match &self.wallet {
            Some(wallet) => Ok(format!("{:?}", wallet.address())),
            None => Err(AdapterError::Configuration(
                "Unlock wallet before use".to_string(),
            )),
        }
    }

    fn sign(&self, state_root: &str) -> AdapterResult<String> {        
        let message = Message::from_slice(&hash_message(state_root));
        match &self.wallet {
            Some(wallet) => {
                let signature = sign(wallet.secret(), &message).expect("sign message");
                println!("{:?}", signature);
                Ok(format!("{}", signature))
            }
            None => Err(AdapterError::Configuration(
                "Unlock the wallet before signing".to_string(),
            )),
        }
    }

    fn verify(&self, signer: &str, state_root: &str, sig: &str) -> AdapterResult<bool> {
        let address = Address::from_slice(signer.as_bytes());
        let signature = match Signature::from_str(sig) {
            Ok(sig) => sig,
            Err(e) => {
                return Err(AdapterError::Signature(
                    "verify: Failed to parse signature".to_string(),
                ))
            }
        };

        let message = Message::from_slice(state_root.as_bytes());

        match verify_address(&address, &signature, &message) {
            Ok(result) => Ok(result),
            Err(e) => Ok(false),
        }
    }

    fn validate_channel(&self, channel: &Channel) -> AdapterResult<bool> {
        let (_eloop, transport) = web3::transports::Http::new(&self.config.ethereum_network)
            .expect("Failed to initialise web3 transport");
        let web3 = web3::Web3::new(transport);
        let contract_address = Address::from_slice(self.config.ethereum_core_address.as_bytes());

        let contract = Contract::from_json(
            web3.eth(),
            contract_address,
            include_bytes!("../contract/AdExCore.json"),
        )
        .expect("failed to initialise contract");

        let eth_channel: EthereumChannel = channel.into();

        let channel_id = eth_channel
            .hash_hex(&self.config.ethereum_core_address)
            .expect("Failed to hash the channel id");

        if channel_id != channel.id {
            return Err(AdapterError::Configuration(
                "channel.id is not valid".to_string(),
            ));
        }

        // @TODO checksum ethereum address
        // check if channel is valid
        let is_channel_valid = EthereumAdapter::is_channel_valid(&self.config, channel);
        if is_channel_valid.is_err() {
            return Err(AdapterError::InvalidChannel(format!(
                "{}",
                is_channel_valid.err().unwrap()
            )));
        }

        // query the blockchain for the channel status
        let contract_query = contract.query("states", channel_id, None, Options::default(), None);
        let channel_status: U256 = contract_query.wait().expect("contract query failed");

        if channel_status != 1.into() {
            return Err(AdapterError::Configuration(
                "channel is not Active on the ethereum network".to_string(),
            ));
        }

        Ok(true)
    }

    fn session_from_token(&mut self, token: &str) -> AdapterResult<Session> {
        let token_id = token.to_owned()[..16].to_string();
        let result = self.tokens_verified.get(&token_id);
        if result.is_some() {
            return Ok(result.unwrap().to_owned());
        }

        let verified = match ewt_verify(&token) {
            Ok(v) => v,
            Err(e) => return Err(AdapterError::EwtVerifyFailed(format!("{}", e))),
        };

        let wallet = match &self.wallet {
            Some(w) => w,
            None => {
                return Err(AdapterError::Configuration(
                    "Failed to unlock wallet".to_string(),
                ))
            }
        };

        let whoami = wallet.public().to_owned();
        if whoami != verified.from {
            return Err(AdapterError::Configuration(
                "token payload.id !== whoami(): token was not intended for us".to_string(),
            ));
        }

        let sess = match &verified.payload.identity {
            Some(identity) => {
                let (_eloop, transport) =
                    web3::transports::Http::new(&self.config.ethereum_network).unwrap();
                let web3 = web3::Web3::new(transport);

                let contract_address =
                    Address::from_slice(self.config.ethereum_core_address.as_bytes());

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
        match (self.tokens_for_auth.get(&validator.id), &self.wallet) {
            (Some(token), Some(_)) => Ok(token.to_owned()),
            (None, Some(wallet)) => {
                let payload = Payload {
                    id: validator.id.clone(),
                    era: usize::try_from(Utc::now().timestamp()).expect("failed to parse utc now"),
                    identity: None,
                    address: None,
                };
                let token = ewt_sign(wallet, &payload).expect("Failed to sign token");
                self.tokens_for_auth
                    .insert(validator.id.clone(), token.clone());
                Ok(token)
            }
            (_, _) => Err(AdapterError::Configuration(
                "failed to unlock wallet".to_string(),
            )),
        }
    }
}

fn hash_message(message: &str) -> [u8; 32] {
    let eth = "\x19Ethereum Signed Message:\n";
    let message_length = message.len();

    let encoded = format!("{}{}{}", eth, message_length, message);

    let mut result = Keccak::new_keccak256();
    result.update(&encoded.as_bytes());

    let mut res: [u8; 32] = [0; 32];
    result.finalize(&mut res);

    res
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
    let parts: Vec<String> = token.split('.').map(ToString::to_string).collect();

    let msg = format!("{}.{}", parts[0], parts[1]);
    let message = Message::from_slice(msg.as_bytes());

    let sig = base64::decode(&parts[2])?;
    let signature = Signature::from_str(&hex::encode(&sig.as_slice())).unwrap();

    let public_key = recover(&signature, &message)?;

    let decode_part1 = base64::decode(&parts[1])?;
    let payload_string = String::from_utf8(decode_part1)?;

    let payload: Payload = serde_json::from_str(&payload_string)?;

    let verified_payload = VerifyPayload {
        from: public_key,
        payload,
    };

    Ok(verified_payload)
}


#[cfg(test)]
mod test {
    use primitives::config::configuration;
    use super::*;


    fn setup_eth_adapter() -> EthereumAdapter {
        let config = configuration("development", None).expect("failed parse config");
        let adapter_options = AdapterOptions {
            keystore_file: Some("./test/resources/keystore.json".to_string()),
            keystore_pwd: Some("adexvalidator".to_string()),
            dummy_identity: None,
            dummy_auth: None,
            dummy_auth_tokens: None,
            
        };

        EthereumAdapter::init(adapter_options, &config)
            .expect("should init ethereum adapter")
    }
    #[test]
    fn should_init_and_unlock_ethereum_adapter() {
        let mut eth_adapter = setup_eth_adapter();
        let unlock = eth_adapter.unlock().expect("should unlock eth adapter");

        assert_eq!(true, unlock, "failed to unlock eth adapter");
    }

    #[test]
    fn should_get_whoami_sign_and_verify_messages() {
        let mut eth_adapter = setup_eth_adapter();
        eth_adapter.unlock().expect("should unlock eth adapter");

        let whoami = eth_adapter.whoami().expect("failed to get whoami");
        println!("whami {}", whoami);
        // assert_eq!(whoami, "0x2bdeafae53940669daa6f519373f686c1f3d3393", "failed to get correct whoami");

        let message = "2bdeafae53940669daa6f519373f686c";
        let expected_response = 
            "b9f9b4b811539f77f48616b551bbc2a085d55e777ca5dab7e0f7b624ec3bd703393805904eb7bc6cbcf89ac84248b33a5498fab238dea57a0715e0a8bb69c63200";
        let response = eth_adapter.sign(message).expect("failed to sign message");
        println!("{}", response);
    }
}