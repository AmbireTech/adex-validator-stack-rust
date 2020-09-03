use async_trait::async_trait;
use primitives::{
    adapter::{
        Adapter, AdapterErrorKind, AdapterResult, DummyAdapterOptions, Error as AdapterError,
        Session,
    },
    channel_validator::ChannelValidator,
    config::Config,
    Channel, ToETHChecksum, ValidatorId,
};
use std::collections::HashMap;
use std::fmt;

#[derive(Debug, Clone)]
pub struct DummyAdapter {
    identity: ValidatorId,
    config: Config,
    // Auth tokens that we have verified (tokenId => session)
    session_tokens: HashMap<String, ValidatorId>,
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
            session_tokens: opts.dummy_auth,
            authorization_tokens: opts.dummy_auth_tokens,
        }
    }
}

#[derive(Debug)]
pub struct Error {}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Dummy Adapter error occurred!")
    }
}

impl AdapterErrorKind for Error {}

#[async_trait]
impl Adapter for DummyAdapter {
    type AdapterError = Error;

    fn unlock(&mut self) -> AdapterResult<(), Self::AdapterError> {
        Ok(())
    }

    fn whoami(&self) -> &ValidatorId {
        &self.identity
    }

    fn sign(&self, state_root: &str) -> AdapterResult<String, Self::AdapterError> {
        let signature = format!(
            "Dummy adapter signature for {} by {}",
            state_root,
            self.whoami().to_checksum()
        );
        Ok(signature)
    }

    fn verify(
        &self,
        signer: &ValidatorId,
        _state_root: &str,
        signature: &str,
    ) -> AdapterResult<bool, Self::AdapterError> {
        // select the `identity` and compare it to the signer
        // for empty string this will return array with 1 element - an empty string `[""]`
        let is_same = match signature.rsplit(' ').take(1).next() {
            Some(from) => from == signer.to_checksum(),
            None => false,
        };

        Ok(is_same)
    }

    async fn validate_channel<'a>(
        &'a self,
        channel: &'a Channel,
    ) -> AdapterResult<bool, Self::AdapterError> {
        DummyAdapter::is_channel_valid(&self.config, self.whoami(), channel)
            .map(|_| true)
            .map_err(AdapterError::InvalidChannel)
    }

    async fn session_from_token<'a>(
        &'a self,
        token: &'a str,
    ) -> AdapterResult<Session, Self::AdapterError> {
        let identity = self
            .authorization_tokens
            .iter()
            .find(|(_, id)| *id == token);

        match identity {
            Some((id, _)) => Ok(Session {
                uid: self.session_tokens[id],
                era: 0,
            }),
            None => Err(AdapterError::Authentication(format!(
                "no session token for this auth: {}",
                token
            ))),
        }
    }

    fn get_auth(&self, _validator: &ValidatorId) -> AdapterResult<String, Self::AdapterError> {
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
