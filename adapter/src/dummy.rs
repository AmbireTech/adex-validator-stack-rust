#![deny(clippy::all)]
#![deny(rust_2018_idioms)]

use primitives::adapter::{Adapter, AdapterOptions, AdapterResult, Session, AdapterError};
use primitives::channel_validator::ChannelValidator;
use primitives::config::Config;
use primitives::{Channel, ValidatorDesc};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct DummyAdapter {
    identity: String,
    tokens_verified: HashMap<String, String>,
    tokens_for_auth: HashMap<String, String>,
}

// Enables DummyAdapter to be able to
// check if a channel is valid
impl ChannelValidator for DummyAdapter {}

impl Adapter for DummyAdapter {
    type Output = DummyAdapter;

    fn init(opts: AdapterOptions, _config: &Config) -> DummyAdapter {
        let identity = opts.dummy_identity.expect("dummyIdentity required");
        let tokens_for_auth = opts.dummy_auth.expect("dummy auth required");
        let tokens_verified = opts.dummy_auth_tokens.expect("dummy auth tokens required");

        Self {
            identity,
            tokens_verified,
            tokens_for_auth,
        }
    }

    fn unlock(&mut self) -> AdapterResult<bool> {
        Ok(true)
    }

    fn whoami(&self) -> String {
        self.identity.to_string()
    }

    fn sign(&self, state_root: &str) -> AdapterResult<String> {
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

    fn validate_channel(&self, channel: &Channel) -> AdapterResult<bool> {
        self.validate_channel(channel)
    }

    fn session_from_token(&mut self, token: &str) -> AdapterResult<Session> {
        let mut identity = "";
        for (key, val) in self.tokens_for_auth.iter() {
            if val == token {
                identity = key;
            }
        }

        Ok(Session { uid: identity.to_owned(), era: 0 })
    }

    fn get_auth(&mut self, _validator: &ValidatorDesc) -> AdapterResult<String> {
        let who = self
            .tokens_verified
            .clone()
            .into_iter()
            .find(|(_, id)| id.to_owned() == self.identity);

        match who {
            Some((id, _)) => {
                let auth = self.tokens_for_auth.get(&id).unwrap();
                Ok(auth.to_owned())
            }
            None => Err(AdapterError::Authentication(format!("no auth token for this identity: {}", self.identity)))
        }
    }
}
