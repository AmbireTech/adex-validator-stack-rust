use primitives::adapter::{Adapter, AdapterError, AdapterResult, Session, DummyAdapterOptions};
use primitives::channel_validator::ChannelValidator;
use primitives::config::Config;
use primitives::Channel;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct DummyAdapter {
    identity: String,
    config: Config,
    // Auth tokens that we have verified (tokenId => session)
    session_tokens: HashMap<String, String>,
    // Auth tokens that we've generated to authenticate with someone (address => token)
    authorization_tokens: HashMap<String, String>,
}

// Enables DummyAdapter to be able to
// check if a channel is valid
impl ChannelValidator for DummyAdapter {}

impl DummyAdapter {
    pub fn init(opts: DummyAdapterOptions, config: &Config) -> Self {
        Self {
            identity: opts.dummy_identity,
            config: config.to_owned(),
            session_tokens: opts.dummy_auth_tokens,
            authorization_tokens: opts.dummy_auth,
        }
    }
}

impl Adapter for DummyAdapter {
    fn unlock(&mut self) -> AdapterResult<()> {
        Ok(())
    }

    fn whoami(&self) -> String {
        self.identity.clone()
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
        match DummyAdapter::is_channel_valid(&self.config, channel) {
            Ok(_) => Ok(true),
            Err(e) => Err(AdapterError::InvalidChannel(e.to_string())),
        }
    }

    fn session_from_token(&mut self, token: &str) -> AdapterResult<Session> {
        let identity = self
            .authorization_tokens
            .iter()
            .find(|(_, id)| *id == token);

        match identity {
            Some((id, _)) => Ok(Session {
                uid: id.to_owned(),
                era: 0,
            }),
            None => Err(AdapterError::Authentication(format!(
                "no session token for this auth: {}",
                token
            ))),
        }
    }

    fn get_auth(&mut self, _validator: &str) -> AdapterResult<String> {
        let who = self
            .session_tokens
            .iter()
            .find(|(_, id)| *id == &self.identity);

        match who {
            Some((id, _)) => {
                let auth = self.authorization_tokens.get(id).expect("id should exist");
                Ok(auth.to_owned())
            }
            None => Err(AdapterError::Authentication(format!(
                "no auth token for this identity: {}",
                self.identity
            ))),
        }
    }
}
