use crate::EthereumChannel;
use chrono::Utc;
use ethkey::{public_to_address, recover, verify_address, Address, Message, Password, Signature};
use ethstore::SafeAccount;
use lazy_static::lazy_static;
use primitives::{
    adapter::{Adapter, AdapterError, AdapterOptions, AdapterResult, Session},
    channel_validator::ChannelValidator,
    config::Config,
    Channel, ValidatorDesc,
};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::HashMap;
use std::error::Error;
use std::fs::File;
use std::path::Path;
use tiny_keccak::Keccak;
use web3::transports::Http;
use web3::{
    contract::{Contract, Options},
    futures::Future,
    types::U256,
};

lazy_static! {
    static ref ADEXCORE_ABI: &'static [u8] =
        include_bytes!("../../lib/protocol-eth/abi/AdExCore.json");
    static ref IDENTITY_ABI: &'static [u8] =
        include_bytes!("../../lib/protocol-eth/abi/Identity.json");
}

#[derive(Debug, Clone)]
pub struct EthereumAdapter {
    keystore_json: String,
    keystore_pwd: Password,
    config: Config,
    // Auth tokens that we have verified (tokenId => session)
    session_tokens: RefCell<HashMap<String, Session>>,
    // Auth tokens that we've generated to authenticate with someone (address => token)
    authorization_tokens: RefCell<HashMap<String, String>>,
    wallet: RefCell<Option<SafeAccount>>,
}

// Enables EthereumAdapter to be able to
// check if a channel is valid
impl ChannelValidator for EthereumAdapter {}

impl Adapter for EthereumAdapter {
    type Output = EthereumAdapter;

    fn init(opts: AdapterOptions, config: &Config) -> AdapterResult<EthereumAdapter> {
        let (keystore_json, keystore_pwd) = match (opts.keystore_file, opts.keystore_pwd) {
            (Some(file), Some(pwd)) => (file, pwd),
            (_, _) => {
                return Err(AdapterError::Configuration(
                    "Missing keystore json file or password".to_string(),
                ))
            }
        };

        Ok(Self {
            keystore_json,
            keystore_pwd: keystore_pwd.into(),
            session_tokens: RefCell::new(HashMap::new()),
            authorization_tokens: RefCell::new(HashMap::new()),
            wallet: RefCell::new(None),
            config: config.to_owned(),
        })
    }

    fn unlock(&self) -> AdapterResult<bool> {
        let json_file = File::open(&Path::new(&self.keystore_json).to_path_buf())
            .map_err(|_| map_error("Invalid keystore location provided"))?;

        let account = SafeAccount::from_file(
            serde_json::from_reader(json_file)
                .map_err(|_| map_error("Invalid keystore json provided"))?,
            None,
            &Some(self.keystore_pwd.clone()),
        )
        .map_err(|_| map_error("Failed to create account"))?;

        self.wallet.replace(Some(account));

        // wallet has been unlocked
        Ok(true)
    }

    fn whoami(&self) -> AdapterResult<String> {
        match self.wallet.borrow().clone() {
            Some(wallet) => {
                let public = wallet
                    .public(&self.keystore_pwd)
                    .map_err(|_| map_error("Failed to get public key"))?;
                let address = format!("{:?}", public_to_address(&public));
                Ok(eth_checksum::checksum(&address))
            }
            None => Err(AdapterError::Configuration(
                "Unlock wallet before use".to_string(),
            )),
        }
    }

    fn sign(&self, state_root: &str) -> AdapterResult<String> {
        let message = Message::from_slice(&hash_message(state_root));
        match self.wallet.borrow().clone() {
            Some(wallet) => {
                let wallet_sign = wallet
                    .sign(&self.keystore_pwd, &message)
                    .map_err(|_| map_error("failed to sign messages"))?;
                let signature: Signature = wallet_sign.into_electrum().into();
                Ok(format!("0x{}", signature))
            }
            None => Err(AdapterError::Configuration(
                "Unlock the wallet before signing".to_string(),
            )),
        }
    }

    fn verify(&self, signer: &str, state_root: &str, sig: &str) -> AdapterResult<bool> {
        let (decoded_adress, decoded_signature) = match (hex::decode(signer), hex::decode(sig)) {
            (Ok(address), Ok(sig)) => (address, sig),
            (_, _) => {
                return Err(AdapterError::Signature(
                    "invalid signature or address".to_string(),
                ))
            }
        };

        let address = Address::from_slice(&decoded_adress);
        let signature = Signature::from_electrum(&decoded_signature);
        let message = Message::from_slice(&hash_message(state_root));

        match verify_address(&address, &signature, &message) {
            Ok(result) => Ok(result),
            Err(_) => Ok(false),
        }
    }

    fn validate_channel(&self, channel: &Channel) -> AdapterResult<bool> {
        let eth_channel: EthereumChannel = channel.into();
        let channel_id = eth_channel
            .hash_hex(&self.config.ethereum_core_address)
            .map_err(|_| map_error("Failed to hash the channel id"))?;

        if channel_id != channel.id {
            return Err(AdapterError::Configuration(
                "channel.id is not valid".to_string(),
            ));
        }

        if check_validator_id_checksum(channel) {
            return Err(AdapterError::Configuration(
                "channel.validators: all addresses are checksummed".to_string(),
            ));
        }
        // check if channel is valid
        let is_channel_valid = EthereumAdapter::is_channel_valid(&self.config, channel);
        if is_channel_valid.is_err() {
            return Err(AdapterError::InvalidChannel(
                is_channel_valid
                    .err()
                    .expect("failed to get channel error")
                    .to_string(),
            ));
        }
        // query the blockchain for the channel status
        let contract_address = Address::from_slice(self.config.ethereum_core_address.as_bytes());
        let contract = get_contract(&self.config, contract_address, &ADEXCORE_ABI)
            .map_err(|_| map_error("failed to init core contract"))?;

        let channel_status: U256 = contract
            .query("states", channel_id, None, Options::default(), None)
            .wait()
            .map_err(|_| map_error("contract channel status query failed"))?;

        if channel_status != 1.into() {
            return Err(AdapterError::Configuration(
                "channel is not Active on the ethereum network".to_string(),
            ));
        }

        Ok(true)
    }

    fn session_from_token(&self, token: &str) -> AdapterResult<Session> {
        let token_id = token.to_owned()[token.len() - 16..].to_string();

        let mut session_tokens = self.session_tokens.borrow_mut();
        if session_tokens.get(&token_id).is_some() {
            return Ok(session_tokens
                .get(&token_id)
                .expect("failed to get session")
                .to_owned());
        }

        let parts: Vec<&str> = token.split('.').collect();
        let (header_encoded, payload_encoded, token_encoded) =
            match (parts.get(0), parts.get(1), parts.get(2)) {
                (Some(header_encoded), Some(payload_encoded), Some(token_encoded)) => {
                    (header_encoded, payload_encoded, token_encoded)
                }
                (_, _, _) => {
                    return Err(AdapterError::Failed(format!(
                        "{} token string is incorrect",
                        token
                    )))
                }
            };

        let verified = match ewt_verify(header_encoded, payload_encoded, token_encoded) {
            Ok(v) => v,
            Err(e) => return Err(AdapterError::EwtVerifyFailed(e.to_string())),
        };

        if self.whoami()? != verified.payload.id {
            return Err(AdapterError::Configuration(
                "token payload.id !== whoami(): token was not intended for us".to_string(),
            ));
        }

        let sess = match &verified.payload.identity {
            Some(identity) => {
                let contract_address = Address::from_slice(identity.as_bytes());
                let contract = get_contract(&self.config, contract_address, &IDENTITY_ABI)
                    .map_err(|_| map_error("failed to init identity contract"))?;

                let priviledge_level: U256 = contract
                    .query("privileges", verified.from, None, Options::default(), None)
                    .wait()
                    .map_err(|_| map_error("failed query priviledge level on contract"))?;

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
                uid: verified.from,
            },
        };

        session_tokens.insert(token_id, sess.clone());
        Ok(sess)
    }

    fn get_auth(&self, validator: &ValidatorDesc) -> AdapterResult<String> {
        let mut authorization_tokens = self.authorization_tokens.borrow_mut();
        match (
            self.wallet.borrow().clone(),
            authorization_tokens.get(&validator.id),
        ) {
            (Some(_), Some(token)) => Ok(token.to_owned()),
            (Some(wallet), None) => {
                let era = Utc::now().timestamp() as f64 / 60000.0;
                let payload = Payload {
                    id: validator.id.clone(),
                    era: era.floor() as i64,
                    identity: None,
                    address: None,
                };
                let token = ewt_sign(&wallet, &self.keystore_pwd, &payload)
                    .map_err(|_| map_error("Failed to sign token"))?;

                authorization_tokens.insert(validator.id.clone(), token.clone());

                Ok(token)
            }
            (_, _) => Err(AdapterError::Configuration(
                "failed to unlock wallet".to_string(),
            )),
        }
    }
}

fn check_validator_id_checksum(channel: &Channel) -> bool {
    let invalid_address_checkum: Vec<&ValidatorDesc> = channel
        .spec
        .validators
        .into_iter()
        .filter(|v| v.id != eth_checksum::checksum(&v.id))
        .collect();

    invalid_address_checkum.is_empty()
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

fn map_error(err: &str) -> AdapterError {
    AdapterError::Failed(err.to_string())
}

fn get_contract(
    config: &Config,
    contract_address: Address,
    abi: &[u8],
) -> Result<Contract<Http>, Box<dyn Error>> {
    let (_eloop, transport) = web3::transports::Http::new(&config.ethereum_network)?;
    let web3 = web3::Web3::new(transport);
    let contract = Contract::from_json(web3.eth(), contract_address, abi)?;

    Ok(contract)
}

// Ethereum Web Tokens
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Payload {
    pub id: String,
    pub era: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub address: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<String>,
}

#[derive(Clone, Debug)]
pub struct VerifyPayload {
    pub from: String,
    pub payload: Payload,
}

#[derive(Serialize, Deserialize)]
struct Header {
    #[serde(rename = "type")]
    header_type: String,
    alg: String,
}

pub fn ewt_sign(
    signer: &SafeAccount,
    password: &Password,
    payload: &Payload,
) -> Result<String, Box<dyn Error>> {
    let header = Header {
        header_type: "JWT".to_string(),
        alg: "ETH".to_string(),
    };

    let header_encoded = base64::encode_config(
        &serde_json::to_string(&header)?.as_bytes(),
        base64::URL_SAFE_NO_PAD,
    );

    let payload_encoded = base64::encode_config(
        &serde_json::to_string(payload)?.as_bytes(),
        base64::URL_SAFE_NO_PAD,
    );

    let message = Message::from_slice(&hash_message(&format!(
        "{}.{}",
        header_encoded, payload_encoded
    )));
    let signature: Signature = signer
        .sign(password, &message)
        .map_err(|_| map_error("sign message"))?
        .into_electrum()
        .into();

    let token = base64::encode_config(
        &hex::decode(format!("{}", signature))?,
        base64::URL_SAFE_NO_PAD,
    );

    Ok(format!("{}.{}.{}", header_encoded, payload_encoded, token))
}

pub fn ewt_verify(
    header_encoded: &str,
    payload_encoded: &str,
    token: &str,
) -> Result<VerifyPayload, Box<dyn Error>> {
    let message = Message::from_slice(&hash_message(&format!(
        "{}.{}",
        header_encoded, payload_encoded
    )));

    let decoded_signature = base64::decode_config(&token, base64::URL_SAFE_NO_PAD)?;
    let signature = Signature::from_electrum(&decoded_signature);

    let address = public_to_address(&recover(&signature, &message)?);

    let payload_string = String::from_utf8(base64::decode_config(
        &payload_encoded,
        base64::URL_SAFE_NO_PAD,
    )?)?;
    let payload: Payload = serde_json::from_str(&payload_string)?;

    let verified_payload = VerifyPayload {
        from: eth_checksum::checksum(&format!("{:?}", address)),
        payload,
    };

    Ok(verified_payload)
}

#[cfg(test)]
mod test {
    use super::*;
    use primitives::config::configuration;

    fn setup_eth_adapter() -> EthereumAdapter {
        let config = configuration("development", None).expect("failed parse config");
        let adapter_options = AdapterOptions {
            keystore_file: Some("./test/resources/keystore.json".to_string()),
            keystore_pwd: Some("adexvalidator".to_string()),
            dummy_identity: None,
            dummy_auth: None,
            dummy_auth_tokens: None,
        };

        EthereumAdapter::init(adapter_options, &config).expect("should init ethereum adapter")
    }

    #[test]
    fn should_init_and_unlock_ethereum_adapter() {
        let eth_adapter = setup_eth_adapter();
        let unlock = eth_adapter.unlock().expect("should unlock eth adapter");

        assert_eq!(true, unlock, "failed to unlock eth adapter");
    }

    #[test]
    fn should_get_whoami_sign_and_verify_messages() {
        // whoami
        let eth_adapter = setup_eth_adapter();
        eth_adapter.unlock().expect("should unlock eth adapter");

        let whoami = eth_adapter.whoami().expect("failed to get whoami");
        assert_eq!(
            whoami, "0x2bDeAFAE53940669DaA6F519373f686c1f3d3393",
            "failed to get correct whoami"
        );

        // Sign
        let expected_response =
            "0xce654de0b3d14d63e1cb3181eee7a7a37ef4a06c9fabc204faf96f26357441b625b1be460fbe8f5278cc02aa88a5d0ac2f238e9e3b8e4893760d33bccf77e47f1b";
        let message = "2bdeafae53940669daa6f519373f686c";
        let response = eth_adapter.sign(message).expect("failed to sign message");
        assert_eq!(expected_response, response, "invalid signature");

        // Verify
        let signature =
            "ce654de0b3d14d63e1cb3181eee7a7a37ef4a06c9fabc204faf96f26357441b625b1be460fbe8f5278cc02aa88a5d0ac2f238e9e3b8e4893760d33bccf77e47f1b";
        let verify = eth_adapter
            .verify(
                "2bDeAFAE53940669DaA6F519373f686c1f3d3393",
                "2bdeafae53940669daa6f519373f686c",
                &signature,
            )
            .expect("Failed to verify signatures");
        assert_eq!(verify, true, "invalid signature verification");
    }

    #[test]
    fn should_generate_correct_ewt_sign_and_verify() {
        let eth_adapter = setup_eth_adapter();
        eth_adapter.unlock().expect("should unlock eth adapter");

        let payload = Payload {
            id: "awesomeValidator".to_string(),
            era: 10_0000,
            address: Some(eth_adapter.whoami().expect("should get whoami ewt sign")),
            identity: None,
        };
        let wallet = eth_adapter.wallet.borrow().clone();
        let response = ewt_sign(&wallet.unwrap(), &eth_adapter.keystore_pwd, &payload)
            .expect("failed to generate ewt signature");
        let expected =
            "eyJ0eXBlIjoiSldUIiwiYWxnIjoiRVRIIn0.eyJpZCI6ImF3ZXNvbWVWYWxpZGF0b3IiLCJlcmEiOjEwMDAwMCwiYWRkcmVzcyI6IjB4MmJEZUFGQUU1Mzk0MDY2OURhQTZGNTE5MzczZjY4NmMxZjNkMzM5MyJ9.gGw_sfnxirENdcX5KJQWaEt4FVRvfEjSLD4f3OiPrJIltRadeYP2zWy9T2GYcK5xxD96vnqAw4GebAW7rMlz4xw";
        assert_eq!(response, expected, "generated wrong ewt signature");

        let expected_verification_response =
            r#"VerifyPayload { from: "0x2bDeAFAE53940669DaA6F519373f686c1f3d3393", payload: Payload { id: "awesomeValidator", era: 100000, address: Some("0x2bDeAFAE53940669DaA6F519373f686c1f3d3393"), identity: None } }"#;

        let parts: Vec<&str> = expected.split('.').collect();
        let verification =
            ewt_verify(parts[0], parts[1], parts[2]).expect("Failed to verify ewt token");

        assert_eq!(
            expected_verification_response,
            format!("{:?}", verification),
            "generated wrong verification payload"
        );
    }
}
