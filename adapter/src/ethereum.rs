use hex::encode;
use std::collections::HashMap;
use std::fmt;
use std::fs;

use futures::future::{err, ok, FutureExt};
use serde::{Deserialize, Serialize};
use web3::futures::Future;
use web3::types::{Address};
use primitives::{Channel};
use primitives::adapter::{Adapter, AdapterFuture, AdapterOptions };
use primitives::config::{Config};
use primitives::channel_validator::{ChannelValidator};

pub struct EthereumAdapter {
    address: Option<Address>,
    keystore_json: String,
    keystore_pwd: String,
    auth_tokens: HashMap<String, String>,
    verified_auth:  HashMap<String, String>,
    wallet: Option<Address>
}

// Enables EthereumAdapter to be able to
// check if a channel is valid
impl ChannelValidator for EthereumAdapter {}

// @TODO
impl Adapter for EthereumAdapter {
    
    type Output = EthereumAdapter;

    fn init(opts: AdapterOptions, config: &Config) -> EthereumAdapter {
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
            wallet: None
        }
    }

    fn unlock(&self) -> AdapterFuture<bool> {
        ok(true).boxed()
    }

    fn whoami(&self) -> String {
        self.address.unwrap().to_string()
    }

    fn sign(&self, state_root: String) -> AdapterFuture<String> {
        let signature = format!(
            "Dummy adapter signature for {} by {}",
            state_root,
            self.whoami()
        );
        ok(signature).boxed()
    }

    fn verify(
        &self,
        signer: &str,
        state_root: &str,
        signature: &str,
    ) -> AdapterFuture<bool> {
        // select the `identity` and compare it to the signer
        // for empty string this will return array with 1 element - an empty string `[""]`
        let is_same = match signature.rsplit(' ').take(1).next() {
            Some(from) => from == signer,
            None => false,
        };

        ok(is_same).boxed()
    }

    fn validate_channel(&self, channel: &Channel) -> AdapterFuture<bool> {
        // @TODO
        ok(true).boxed()
    }

    fn session_from_token(&self, token: &str) -> AdapterFuture<String> {
        // @TODO
        ok("hello".to_string()).boxed()
    }

    fn get_auth(&self, validator: &str) -> AdapterFuture<String> {
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
       ok("auth".to_string()).boxed()
    }
}