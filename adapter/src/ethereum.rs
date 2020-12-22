use crate::EthereumChannel;
use async_trait::async_trait;
use chrono::Utc;
use error::*;
use ethstore::{
    ethkey::{public_to_address, recover, verify_address, Address, Message, Password, Signature},
    SafeAccount,
};
use futures::TryFutureExt;
use lazy_static::lazy_static;
use primitives::{
    adapter::{Adapter, AdapterResult, Error as AdapterError, KeystoreOptions, Session},
    channel_validator::ChannelValidator,
    config::Config,
    Channel, ChannelId, ToETHChecksum, ValidatorId,
};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::convert::TryFrom;
use std::fs;
use tiny_keccak::Keccak;
use web3::{
    contract::tokens::Tokenizable,
    contract::{Contract, Options},
    transports::Http,
    types::{H256, U256},
    Web3,
};

mod error;

lazy_static! {
    static ref ADEXCORE_ABI: &'static [u8] =
        include_bytes!("../../lib/protocol-eth/abi/AdExCore.json");
    static ref CHANNEL_STATE_ACTIVE: U256 = 1.into();
}

#[derive(Debug, Clone)]
pub struct EthereumAdapter {
    address: ValidatorId,
    keystore_json: Value,
    keystore_pwd: Password,
    config: Config,
    wallet: Option<SafeAccount>,
    web3: Web3<Http>,
    relayer: RelayerClient,
}

// Enables EthereumAdapter to be able to
// check if a channel is valid
impl ChannelValidator for EthereumAdapter {}

impl EthereumAdapter {
    pub fn init(opts: KeystoreOptions, config: &Config) -> AdapterResult<EthereumAdapter, Error> {
        let keystore_contents =
            fs::read_to_string(&opts.keystore_file).map_err(KeystoreError::ReadingFile)?;

        let keystore_json: Value =
            serde_json::from_str(&keystore_contents).map_err(KeystoreError::Deserialization)?;

        let address = keystore_json["address"]
            .as_str()
            .map(eth_checksum::checksum)
            .ok_or(KeystoreError::AddressMissing)?;

        let address = ValidatorId::try_from(&address).map_err(KeystoreError::AddressInvalid)?;

        let transport =
            web3::transports::Http::new(&config.ethereum_network).map_err(Error::Web3)?;
        let web3 = web3::Web3::new(transport);
        let relayer =
            RelayerClient::new(&config.ethereum_adapter_relayer).map_err(Error::RelayerClient)?;

        Ok(Self {
            address,
            keystore_json,
            keystore_pwd: opts.keystore_pwd.into(),
            wallet: None,
            config: config.to_owned(),
            web3,
            relayer,
        })
    }
}

#[async_trait]
impl Adapter for EthereumAdapter {
    type AdapterError = Error;

    fn unlock(&mut self) -> AdapterResult<(), Self::AdapterError> {
        let account = SafeAccount::from_file(
            serde_json::from_value(self.keystore_json.clone())
                .map_err(KeystoreError::Deserialization)?,
            None,
            &Some(self.keystore_pwd.clone()),
        )
        .map_err(Error::WalletUnlock)?;

        self.wallet = Some(account);

        Ok(())
    }

    fn whoami(&self) -> &ValidatorId {
        &self.address
    }

    fn sign(&self, state_root: &str) -> AdapterResult<String, Self::AdapterError> {
        if let Some(wallet) = &self.wallet {
            let state_root = hex::decode(state_root).map_err(VerifyError::StateRootDecoding)?;
            let message = Message::from(hash_message(&state_root));
            let wallet_sign = wallet
                .sign(&self.keystore_pwd, &message)
                .map_err(EwtSigningError::SigningMessage)?;
            let signature: Signature = wallet_sign.into_electrum().into();

            Ok(format!("0x{}", signature))
        } else {
            Err(AdapterError::LockedWallet)
        }
    }

    /// `state_root` is hex string which **should not** be `0x` prefixed
    /// `sig` is hex string wihch **should be** `0x` prefixed
    fn verify(
        &self,
        signer: &ValidatorId,
        state_root: &str,
        sig: &str,
    ) -> AdapterResult<bool, Self::AdapterError> {
        if !sig.starts_with("0x") {
            return Err(VerifyError::SignatureNotPrefixed.into());
        }
        let decoded_signature = hex::decode(&sig[2..]).map_err(VerifyError::SignatureDecoding)?;
        let address = Address::from(*signer.inner());
        let signature = Signature::from_electrum(&decoded_signature);
        let state_root = hex::decode(state_root).map_err(VerifyError::StateRootDecoding)?;
        let message = Message::from(hash_message(&state_root));

        let verify_address = verify_address(&address, &signature, &message)
            .map_err(VerifyError::PublicKeyRecovery)?;

        Ok(verify_address)
    }

    async fn validate_channel<'a>(
        &'a self,
        channel: &'a Channel,
    ) -> AdapterResult<bool, Self::AdapterError> {
        // check if channel is valid
        EthereumAdapter::is_channel_valid(&self.config, self.whoami(), channel)
            .map_err(AdapterError::InvalidChannel)?;

        let eth_channel =
            EthereumChannel::try_from(channel).map_err(AdapterError::InvalidChannel)?;

        let eth_channel_id = ChannelId::from(eth_channel.hash(&self.config.ethereum_core_address));

        if eth_channel_id != channel.id {
            return Err(AdapterError::Adapter(
                Error::InvalidChannelId {
                    expected: eth_channel_id,
                    actual: channel.id,
                }
                .into(),
            ));
        }

        let contract = Contract::from_json(
            self.web3.eth(),
            self.config.ethereum_core_address.into(),
            &ADEXCORE_ABI,
        )
        .map_err(Error::ContractInitialization)?;

        let channel_status: U256 = contract
            .query(
                "states",
                H256(*channel.id).into_token(),
                None,
                Options::default(),
                None,
            )
            .await
            .map_err(Error::ContractQuerying)?;

        if channel_status != *CHANNEL_STATE_ACTIVE {
            Err(AdapterError::Adapter(
                Error::ChannelInactive(channel.id).into(),
            ))
        } else {
            Ok(true)
        }
    }

    /// Creates a `Session` from a provided Token by calling the Contract.
    /// Does **not** cache the (`Token`, `Session`) pair.
    async fn session_from_token<'a>(
        &'a self,
        token: &'a str,
    ) -> AdapterResult<Session, Self::AdapterError> {
        if token.len() < 16 {
            return Err(AdapterError::Authentication(
                "Invalid token id length".to_string(),
            ));
        }

        let parts: Vec<&str> = token.split('.').collect();
        let (header_encoded, payload_encoded, token_encoded) =
            match (parts.get(0), parts.get(1), parts.get(2)) {
                (Some(header_encoded), Some(payload_encoded), Some(token_encoded)) => {
                    (header_encoded, payload_encoded, token_encoded)
                }
                _ => {
                    return Err(AdapterError::Authentication(format!(
                        "{} token string is incorrect",
                        token
                    )))
                }
            };

        let verified = ewt_verify(header_encoded, payload_encoded, token_encoded)
            .map_err(Error::VerifyMessage)?;

        if self.whoami().to_checksum() != verified.payload.id {
            return Err(AdapterError::Authentication(
                "token payload.id !== whoami(): token was not intended for us".to_string(),
            ));
        }

        let sess = match &verified.payload.identity {
            Some(identity) => {
                if self
                    .relayer
                    .has_privileges(&verified.from, identity)
                    .await?
                {
                    Session {
                        era: verified.payload.era,
                        uid: identity.to_owned(),
                    }
                } else {
                    return Err(AdapterError::Authorization(
                        "insufficient privilege".to_string(),
                    ));
                }
            }
            None => Session {
                era: verified.payload.era,
                uid: verified.from,
            },
        };

        Ok(sess)
    }

    fn get_auth(&self, validator: &ValidatorId) -> AdapterResult<String, Self::AdapterError> {
        let wallet = self.wallet.as_ref().ok_or(AdapterError::LockedWallet)?;

        let era = Utc::now().timestamp_millis() as f64 / 60000.0;
        let payload = Payload {
            id: validator.to_checksum(),
            era: era.floor() as i64,
            identity: None,
            address: self.whoami().to_checksum(),
        };

        ewt_sign(&wallet, &self.keystore_pwd, &payload)
            .map_err(|err| AdapterError::Adapter(Error::SignMessage(err).into()))
    }
}

#[derive(Debug, Clone)]
struct RelayerClient {
    client: Client,
    relayer_url: String,
}

impl RelayerClient {
    pub fn new(relayer_url: &str) -> Result<Self, reqwest::Error> {
        let client = Client::builder().build()?;

        Ok(Self {
            relayer_url: relayer_url.to_string(),
            client,
        })
    }

    /// Checks whether there are any privileges (i.e. > 0)
    pub async fn has_privileges(
        &self,
        from: &ValidatorId,
        identity: &ValidatorId,
    ) -> Result<bool, AdapterError<Error>> {
        use reqwest::Response;
        use std::collections::HashMap;
        let relay_url = format!(
            "{}/identity/by-owner/{}",
            self.relayer_url,
            from.to_checksum()
        );

        let identities_owned: HashMap<ValidatorId, u8> = self
            .client
            .get(&relay_url)
            .send()
            .and_then(|res: Response| res.json())
            .await
            .map_err(Error::RelayerClient)?;

        let has_privileges = identities_owned
            .get(identity)
            .map_or(false, |privileges| *privileges > 0);
        Ok(has_privileges)
    }
}

fn hash_message(message: &[u8]) -> [u8; 32] {
    let eth = "\x19Ethereum Signed Message:\n";
    let message_length = message.len();

    let mut result = Keccak::new_keccak256();
    result.update(&format!("{}{}", eth, message_length).as_bytes());
    result.update(&message);

    let mut res: [u8; 32] = [0; 32];
    result.finalize(&mut res);

    res
}

// Ethereum Web Tokens
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Payload {
    pub id: String,
    pub era: i64,
    pub address: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<ValidatorId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifyPayload {
    pub from: ValidatorId,
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
) -> Result<String, EwtSigningError> {
    let header = Header {
        header_type: "JWT".to_string(),
        alg: "ETH".to_string(),
    };

    let header_encoded = base64::encode_config(
        &serde_json::to_string(&header).map_err(EwtSigningError::HeaderSerialization)?,
        base64::URL_SAFE_NO_PAD,
    );

    let payload_encoded = base64::encode_config(
        &serde_json::to_string(payload).map_err(EwtSigningError::PayloadSerialization)?,
        base64::URL_SAFE_NO_PAD,
    );
    let message = Message::from(hash_message(
        &format!("{}.{}", header_encoded, payload_encoded).as_bytes(),
    ));
    let signature: Signature = signer
        .sign(password, &message)
        .map_err(EwtSigningError::SigningMessage)?
        .into_electrum()
        .into();

    let token = base64::encode_config(
        &hex::decode(format!("{}", signature)).map_err(EwtSigningError::DecodingHexSignature)?,
        base64::URL_SAFE_NO_PAD,
    );

    Ok(format!("{}.{}.{}", header_encoded, payload_encoded, token))
}

pub fn ewt_verify(
    header_encoded: &str,
    payload_encoded: &str,
    token: &str,
) -> Result<VerifyPayload, EwtVerifyError> {
    let message = Message::from(hash_message(
        &format!("{}.{}", header_encoded, payload_encoded).as_bytes(),
    ));

    let decoded_signature = base64::decode_config(&token, base64::URL_SAFE_NO_PAD)
        .map_err(EwtVerifyError::SignatureDecoding)?;
    let signature = Signature::from_electrum(&decoded_signature);

    let address =
        public_to_address(&recover(&signature, &message).map_err(EwtVerifyError::AddressRecovery)?);

    let payload_string = String::from_utf8(
        base64::decode_config(&payload_encoded, base64::URL_SAFE_NO_PAD)
            .map_err(EwtVerifyError::PayloadDecoding)?,
    )
    .map_err(EwtVerifyError::PayloadUtf8)?;
    let payload: Payload =
        serde_json::from_str(&payload_string).map_err(EwtVerifyError::PayloadDeserialization)?;

    let verified_payload = VerifyPayload {
        from: ValidatorId::from(&address.0),
        payload,
    };

    Ok(verified_payload)
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::EthereumChannel;
    use chrono::{Duration, Utc};
    use hex::FromHex;
    use primitives::config::configuration;
    use primitives::ChannelId;
    use primitives::{adapter::KeystoreOptions, targeting::Rules};
    use primitives::{ChannelSpec, EventSubmission, SpecValidators, ValidatorDesc};
    use std::convert::TryFrom;
    use web3::types::Address;
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };

    fn setup_eth_adapter(contract_address: Option<[u8; 20]>) -> EthereumAdapter {
        let mut config = configuration("development", None).expect("failed parse config");
        let keystore_options = KeystoreOptions {
            keystore_file: "./test/resources/keystore.json".to_string(),
            keystore_pwd: "adexvalidator".to_string(),
        };

        if let Some(ct_address) = contract_address {
            config.ethereum_core_address = ct_address;
        }

        EthereumAdapter::init(keystore_options, &config).expect("should init ethereum adapter")
    }

    #[test]
    fn should_init_and_unlock_ethereum_adapter() {
        let mut eth_adapter = setup_eth_adapter(None);
        eth_adapter.unlock().expect("should unlock eth adapter");
    }

    #[test]
    fn should_get_whoami_sign_and_verify_messages() {
        // whoami
        let mut eth_adapter = setup_eth_adapter(None);
        let whoami = eth_adapter.whoami();
        assert_eq!(
            whoami.to_string(),
            "0x2bDeAFAE53940669DaA6F519373f686c1f3d3393",
            "failed to get correct whoami"
        );

        eth_adapter.unlock().expect("should unlock eth adapter");

        // Sign
        let expected_response =
            "0x625fd46f82c4cfd135ea6a8534e85dbf50beb157046dce59d2e97aacdf4e38381d1513c0e6f002b2f05c05458038b187754ff38cc0658dfc9ba854cccfb6e13e1b";
        let message = "2bdeafae53940669daa6f519373f686c";
        let signature = eth_adapter.sign(message).expect("failed to sign message");
        assert_eq!(expected_response, signature, "invalid signature");

        // Verify
        let signature =
            "0x9e07f12958ce7c5eb1362eb9461e4745dd9d74a42b921391393caea700bfbd6e1ad876a7d8f9202ef1fe6110dbfe87840c5676ca5c4fda9f3330694a1ac2a1fc1b";
        let verify = eth_adapter
            .verify(
                &ValidatorId::try_from("2892f6C41E0718eeeDd49D98D648C789668cA67d")
                    .expect("Failed to parse id"),
                "8bc45d8eb27f4c98cab35d17b0baecc2a263d6831ef0800f4c190cbfac6d20a3",
                &signature,
            )
            .expect("Failed to verify signatures");

        let signature2 = "0x9fa5852041b9818021323aff8260624fd6998c52c95d9ad5036e0db6f2bf2b2d48a188ec1d638581ff56b0a2ecceca6d3880fc65030558bd8f68b154e7ebf80f1b";
        let message2 = "1648231285e69677531ffe70719f67a07f3d4393b8425a5a1c84b0c72434c77b";

        let verify2 = eth_adapter
            .verify(
                &ValidatorId::try_from("ce07CbB7e054514D590a0262C93070D838bFBA2e")
                    .expect("Failed to parse id"),
                message2,
                &signature2,
            )
            .expect("Failed to verify signatures");

        assert!(verify, "invalid signature 1 verification");
        assert!(verify2, "invalid signature 2 verification");
    }

    #[test]
    fn should_generate_correct_ewt_sign_and_verify() {
        let mut eth_adapter = setup_eth_adapter(None);
        eth_adapter.unlock().expect("should unlock eth adapter");

        let payload = Payload {
            id: "awesomeValidator".into(),
            era: 100_000,
            address: eth_adapter.whoami().to_checksum(),
            identity: None,
        };
        let wallet = eth_adapter.wallet.clone();
        let response = ewt_sign(&wallet.unwrap(), &eth_adapter.keystore_pwd, &payload)
            .expect("failed to generate ewt signature");
        let expected = "eyJ0eXBlIjoiSldUIiwiYWxnIjoiRVRIIn0.eyJpZCI6ImF3ZXNvbWVWYWxpZGF0b3IiLCJlcmEiOjEwMDAwMCwiYWRkcmVzcyI6IjB4MmJEZUFGQUU1Mzk0MDY2OURhQTZGNTE5MzczZjY4NmMxZjNkMzM5MyJ9.gGw_sfnxirENdcX5KJQWaEt4FVRvfEjSLD4f3OiPrJIltRadeYP2zWy9T2GYcK5xxD96vnqAw4GebAW7rMlz4xw";
        assert_eq!(response, expected, "generated wrong ewt signature");

        let expected_verification_response = VerifyPayload {
            from: ValidatorId::try_from("0x2bdeafae53940669daa6f519373f686c1f3d3393")
                .expect("Valid ValidatorId"),
            payload: Payload {
                id: "awesomeValidator".to_string(),
                era: 100_000,
                address: "0x2bDeAFAE53940669DaA6F519373f686c1f3d3393".to_string(),
                identity: None,
            },
        };

        let parts: Vec<&str> = expected.split('.').collect();
        let verification =
            ewt_verify(parts[0], parts[1], parts[2]).expect("Failed to verify ewt token");

        assert_eq!(
            expected_verification_response, verification,
            "generated wrong verification payload"
        );
    }

    #[tokio::test]
    async fn test_session_from_token() {
        use primitives::ToETHChecksum;
        use std::collections::HashMap;

        let identity = ValidatorId::try_from("0x5B04DBc513F90CaAFAa09307Ad5e3C65EB4b26F0").unwrap();
        let server = MockServer::start().await;
        let mut identities_owned: HashMap<ValidatorId, u8> = HashMap::new();
        identities_owned.insert(identity, 2);

        let mut eth_adapter = setup_eth_adapter(None);

        Mock::given(method("GET"))
            .and(path(format!("/identity/by-owner/{}", eth_adapter.whoami())))
            .respond_with(ResponseTemplate::new(200).set_body_json(&identities_owned))
            .mount(&server)
            .await;

        eth_adapter.unlock().expect("should unlock eth adapter");
        let wallet = eth_adapter.wallet.clone();

        let era = Utc::now().timestamp_millis() as f64 / 60000.0;
        let payload = Payload {
            id: eth_adapter.whoami().to_checksum(),
            era: era.floor() as i64,
            identity: Some(identity.clone()),
            address: eth_adapter.whoami().to_checksum(),
        };

        let token = ewt_sign(&wallet.unwrap(), &eth_adapter.keystore_pwd, &payload).unwrap();

        let session: Session = eth_adapter.session_from_token(&token).await.unwrap();

        assert_eq!(session.uid, identity);
    }

    #[tokio::test]
    async fn should_validate_valid_channel_properly() {
        let http =
            web3::transports::Http::new("http://localhost:8545").expect("failed to init transport");

        let web3 = web3::Web3::new(http);
        let leader_account: Address = "Df08F82De32B8d460adbE8D72043E3a7e25A3B39"
            .parse()
            .expect("failed to parse leader account");
        let _follower_account: Address = "6704Fbfcd5Ef766B287262fA2281C105d57246a6"
            .parse()
            .expect("failed to parse leader account");

        // tokenbytecode.json
        let token_bytecode = include_str!("../test/resources/tokenbytecode.json");
        // token_abi.json
        let token_abi = include_bytes!("../test/resources/tokenabi.json");
        // adexbytecode.json
        let adex_bytecode = include_str!("../../lib/protocol-eth/resources/bytecode/AdExCore.json");

        // deploy contracts
        let token_contract = Contract::deploy(web3.eth(), token_abi)
            .expect("invalid token token contract")
            .confirmations(0)
            .options(Options::with(|opt| {
                opt.gas_price = Some(1.into());
                opt.gas = Some(6_721_975.into());
            }))
            .execute(token_bytecode, (), leader_account)
            .await
            .expect("Correct parameters are passed to the constructor.");

        let adex_contract = Contract::deploy(web3.eth(), &ADEXCORE_ABI)
            .expect("invalid adex contract")
            .confirmations(0)
            .options(Options::with(|opt| {
                opt.gas_price = Some(1.into());
                opt.gas = Some(6_721_975.into());
            }))
            .execute(adex_bytecode, (), leader_account)
            .await
            .expect("Correct parameters are passed to the constructor.");

        // contract call set balance
        token_contract
            .call(
                "setBalanceTo",
                (Address::from(leader_account), U256::from(2000_u64)),
                leader_account,
                Options::default(),
            )
            .await
            .expect("Failed to set balance");

        let leader_validator_desc = ValidatorDesc {
            // keystore.json address (same with js)
            id: ValidatorId::try_from("2bdeafae53940669daa6f519373f686c1f3d3393")
                .expect("failed to create id"),
            url: "http://localhost:8005".to_string(),
            fee: 100.into(),
            fee_addr: None,
        };

        let follower_validator_desc = ValidatorDesc {
            // keystore2.json address (same with js)
            id: ValidatorId::try_from("6704Fbfcd5Ef766B287262fA2281C105d57246a6")
                .expect("failed to create id"),
            url: "http://localhost:8006".to_string(),
            fee: 100.into(),
            fee_addr: None,
        };

        let mut valid_channel = Channel {
            // to be replace with the proper id
            id: ChannelId::from_hex(
                "061d5e2a67d0a9a10f1c732bca12a676d83f79663a396f7d87b3e30b9b411088",
            )
            .expect("prep_db: failed to deserialize channel id"),
            // leader_account
            creator: ValidatorId::try_from("Df08F82De32B8d460adbE8D72043E3a7e25A3B39")
                .expect("should be valid ValidatorId"),
            deposit_asset: eth_checksum::checksum(&format!("{:?}", token_contract.address())),
            deposit_amount: 2_000.into(),
            valid_until: Utc::now() + Duration::days(2),
            targeting_rules: Rules::new(),
            spec: ChannelSpec {
                title: None,
                validators: SpecValidators::new(leader_validator_desc, follower_validator_desc),
                max_per_impression: 10.into(),
                min_per_impression: 10.into(),
                targeting_rules: Rules::new(),
                event_submission: Some(EventSubmission { allow: vec![] }),
                created: Utc::now(),
                active_from: None,
                nonce: None,
                withdraw_period_start: Utc::now() + Duration::days(1),
                ad_units: vec![],
                pricing_bounds: None,
            },
            exhausted: Default::default(),
        };

        // convert to eth channel
        let eth_channel =
            EthereumChannel::try_from(&valid_channel).expect("failed to create eth channel");
        let sol_tuple = eth_channel.to_solidity_tuple();

        // contract call open channel
        adex_contract
            .call(
                "channelOpen",
                (sol_tuple,),
                leader_account,
                Options::default(),
            )
            .await
            .expect("open channel");

        let contract_addr = adex_contract.address().to_fixed_bytes();
        let channel_id = eth_channel.hash(&contract_addr);
        // set id to proper id
        valid_channel.id = ChannelId::from(channel_id);

        // eth adapter
        let mut eth_adapter = setup_eth_adapter(Some(contract_addr));
        eth_adapter.unlock().expect("should unlock eth adapter");
        // validate channel
        let result = eth_adapter
            .validate_channel(&valid_channel)
            .await
            .expect("failed to validate channel");

        assert!(result, "should validate valid channel correctly");
    }
}
