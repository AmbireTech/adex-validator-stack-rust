#![deny(clippy::all)]
#![deny(rust_2018_idioms)]

use futures::future::{ok, FutureExt};
use primitives::adapter::{Adapter, AdapterOptions, AdapterResult};
use primitives::channel_validator::ChannelValidator;
use primitives::config::Config;
use primitives::{Channel, ValidatorDesc};
use std::collections::HashMap;
use web3::types::Address;

#[derive(Debug, Clone)]
pub struct EthereumAdapter {
    address: Option<Address>,
    keystore_json: String,
    keystore_pwd: String,
    auth_tokens: HashMap<String, String>,
    verified_auth: HashMap<String, String>,
    wallet: Option<Address>,
}

// Enables EthereumAdapter to be able to
// check if a channel is valid
impl ChannelValidator for EthereumAdapter {}

// @TODO
impl Adapter for EthereumAdapter {
    type Output = EthereumAdapter;

    fn init(opts: AdapterOptions, _config: &Config) -> EthereumAdapter {
        // opts.dummy_identity.expect("dummyIdentity required");
        // opts.dummy_auth.expect("dummy auth required");
        // opts.dummy_auth_tokens.expect("dummy auth tokens required");
        // self.identity = opts.dummy_identity.unwrap();
        // self.authTokens = opts.dummy_auth.unwrap();
        // self.verifiedAuth = opts.dummy_auth_tokens.unwrap();
        let keystore_json = opts.keystore_file.unwrap();
        let keystore_pwd = opts.keystore_pwd.unwrap();

        Self {
            address: None,
            keystore_json,
            keystore_pwd,
            auth_tokens: HashMap::new(),
            verified_auth: HashMap::new(),
            wallet: None,
        }
    }

    fn unlock(&self) -> AdapterResult<bool> {
        Ok(true)
    }

    fn whoami(&self) -> String {
        self.address.unwrap().to_string()
    }

    fn sign(&self, state_root: String) -> AdapterResult<String> {
        let signature = format!(
            "Dummy adapter signature for {} by {}",
            state_root,
            self.whoami()
        );
        Ok(signature)
    }

    fn verify(&self, signer: &str, _state_root: &str, signature: &str) -> AdapterResult<bool> {
        // select the `identity` and compare it to the signer
        // for empty string this will return array with 1 element - an empty string `[""]`
        let is_same = match signature.rsplit(' ').take(1).next() {
            Some(from) => from == signer,
            None => false,
        };

        Ok(is_same)
    }

    fn validate_channel(&self, _channel: &Channel) -> AdapterResult<bool> {
        // @TODO
        Ok(true)
    }

    fn session_from_token(&self, _token: &str) -> AdapterResult<String> {
        // @TODO
        Ok("hello".to_string())
    }

    fn get_auth(&self, _validator: &ValidatorDesc) -> AdapterResult<String> {
        // let participant = self
        //     .participants
        //     .iter()
        //     .find(|&(_, participant)| participant.identity == validator);
        // let future = match participant {
        //     Some((_, participant)) => ok(participant.token.to_string()),
        //     None => err(AdapterError::Authentication(
        //         "Identity not found".to_string(),
        //     )),
        // };
        Ok("auth".to_string())
    }
}
