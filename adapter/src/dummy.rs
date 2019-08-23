#![deny(clippy::all)]
#![deny(rust_2018_idioms)]

use futures::future::{ok, FutureExt};
use primitives::adapter::{Adapter, AdapterFuture, AdapterOptions};
use primitives::channel_validator::ChannelValidator;
use primitives::config::Config;
use primitives::Channel;
use std::collections::HashMap;

pub struct DummyAdapter {
    identity: String,
    auth_tokens: HashMap<String, String>,
    verified_auth: HashMap<String, String>,
}

// Enables DummyAdapter to be able to
// check if a channel is valid
impl ChannelValidator for DummyAdapter {}

impl Adapter for DummyAdapter {
    type Output = DummyAdapter;

    fn init(opts: AdapterOptions, _config: &Config) -> DummyAdapter {
        // opts.dummy_identity.expect("dummyIdentity required");
        // opts.dummy_auth.expect("dummy auth required");
        // opts.dummy_auth_tokens.expect("dummy auth tokens required");
        // self.identity = opts.dummy_identity.unwrap();
        // self.authTokens = opts.dummy_auth.unwrap();
        // self.verifiedAuth = opts.dummy_auth_tokens.unwrap();
        Self {
            identity: opts.dummy_identity.unwrap(),
            auth_tokens: HashMap::new(),
            verified_auth: HashMap::new(),
        }
    }

    fn unlock(&self) -> AdapterFuture<bool> {
        ok(true).boxed()
    }

    fn whoami(&self) -> String {
        self.identity.to_string()
    }

    fn sign(&self, state_root: String) -> AdapterFuture<String> {
        let signature = format!(
            "Dummy adapter signature for {} by {}",
            state_root,
            self.whoami()
        );
        ok(signature).boxed()
    }

    fn verify(&self, signer: &str, _state_root: &str, signature: &str) -> AdapterFuture<bool> {
        // select the `identity` and compare it to the signer
        // for empty string this will return array with 1 element - an empty string `[""]`
        let is_same = match signature.rsplit(' ').take(1).next() {
            Some(from) => from == signer,
            None => false,
        };

        ok(is_same).boxed()
    }

    fn validate_channel(&self, _channel: &Channel) -> AdapterFuture<bool> {
        // @TODO
        ok(true).boxed()
    }

    fn session_from_token(&self, _token: &str) -> AdapterFuture<String> {
        // @TODO
        ok("hello".to_string()).boxed()
    }

    fn get_auth(&self, _validator: &str) -> AdapterFuture<String> {
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
