use primitives::adapter::{Adapter, AdapterError, AdapterOptions, AdapterResult, Session};
use primitives::channel_validator::ChannelValidator;
use primitives::config::Config;
use primitives::{Channel, ValidatorDesc};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct DummyAdapter {
    identity: String,
    config: Config,
    tokens_verified: HashMap<String, String>,
    tokens_for_auth: HashMap<String, String>,
}

// Enables DummyAdapter to be able to
// check if a channel is valid
impl ChannelValidator for DummyAdapter {}

impl Adapter for DummyAdapter {
    type Output = DummyAdapter;

    fn init(opts: AdapterOptions, config: &Config) -> AdapterResult<DummyAdapter> {
        let (identity, tokens_for_auth, tokens_verified) =
            match (opts.dummy_identity, opts.dummy_auth, opts.dummy_auth_tokens) {
                (Some(identity), Some(tokens_for_auth), Some(tokens_verified)) => {
                    (identity, tokens_for_auth, tokens_verified)
                }
                (_, _, _) => {
                    return Err(AdapterError::Configuration(
                        "dummy_identity, dummy_auth, dummy_auth_tokens required".to_string(),
                    ))
                }
            };

        Ok(Self {
            identity,
            config: config.to_owned(),
            tokens_verified,
            tokens_for_auth,
        })
    }

    fn unlock(&self) -> AdapterResult<bool> {
        Ok(true)
    }

    fn whoami(&self) -> AdapterResult<String> {
        Ok(self.identity.to_string())
    }

    fn sign(&self, state_root: &str) -> AdapterResult<String> {
        let signature = format!(
            "Dummy adapter signature for {} by {}",
            state_root,
            self.whoami()?
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

    fn session_from_token(&self, token: &str) -> AdapterResult<Session> {
        let mut identity = "";
        for (key, val) in self.tokens_for_auth.iter() {
            if val == token {
                identity = key;
            }
        }

        Ok(Session {
            uid: identity.to_owned(),
            era: 0,
        })
    }

    fn get_auth(&self, _validator: &ValidatorDesc) -> AdapterResult<String> {
        let who = self
            .tokens_verified
            .clone()
            .into_iter()
            .find(|(_, id)| *id == self.identity);

        match who {
            Some((id, _)) => {
                let auth = self.tokens_for_auth.get(&id).unwrap();
                Ok(auth.to_owned())
            }
            None => Err(AdapterError::Authentication(format!(
                "no auth token for this identity: {}",
                self.identity
            ))),
        }
    }
}
