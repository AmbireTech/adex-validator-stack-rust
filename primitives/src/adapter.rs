use crate::channel::ChannelError;
use crate::channel_validator::ChannelValidator;
use crate::{Channel, DomainError, ValidatorId};
use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::From;
use std::error::Error;
use std::fmt;

pub type AdapterResult<T, AE> = Result<T, AdapterError<AE>>;

pub trait AdapterErrorKind: fmt::Debug + fmt::Display {}

#[derive(Debug)]
pub enum AdapterError<AE: AdapterErrorKind> {
    Authentication(String),
    Authorization(String),
    Configuration(String),
    Signature(String),
    InvalidChannel(ChannelError),
    Failed(String),
    /// Adapter specific errors
    Adapter(AE),
    Domain(DomainError),
    /// If
    LockedWallet,
}

impl<AE: AdapterErrorKind> Error for AdapterError<AE> {}

impl<AE: AdapterErrorKind> From<AE> for AdapterError<AE> {
    fn from(adapter_err: AE) -> Self {
        Self::Adapter(adapter_err)
    }
}

impl<AE: AdapterErrorKind> fmt::Display for AdapterError<AE> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AdapterError::Authentication(error) => write!(f, "Authentication error: {}", error),
            AdapterError::Authorization(error) => write!(f, "Authorization error: {}", error),
            AdapterError::Configuration(error) => write!(f, "Configuration error: {}", error),
            AdapterError::Signature(error) => write!(f, "Signature error: {}", error),
            AdapterError::InvalidChannel(error) => write!(f, "{}", error),
            AdapterError::Failed(error) => write!(f, "error: {}", error),
            AdapterError::Adapter(error) => write!(f, "Adapter specific error: {}", error),
            AdapterError::Domain(error) => write!(f, "Domain error: {}", error),
            AdapterError::LockedWallet => write!(f, "You must `.unlock()` the wallet first"),
        }
    }
}

impl<AE: AdapterErrorKind> From<DomainError> for AdapterError<AE> {
    fn from(err: DomainError) -> AdapterError<AE> {
        AdapterError::Domain(err)
    }
}

pub struct DummyAdapterOptions {
    pub dummy_identity: ValidatorId,
    pub dummy_auth: HashMap<String, ValidatorId>,
    pub dummy_auth_tokens: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct KeystoreOptions {
    pub keystore_file: String,
    pub keystore_pwd: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub era: i64,
    pub uid: ValidatorId,
}

pub trait Adapter: ChannelValidator + Send + Sync + Clone + fmt::Debug {
    type AdapterError: AdapterErrorKind;

    /// Unlock adapter
    fn unlock(&mut self) -> AdapterResult<(), Self::AdapterError>;

    /// Get Adapter whoami
    fn whoami(&self) -> &ValidatorId;

    /// Signs the provided state_root
    fn sign(&self, state_root: &str) -> AdapterResult<String, Self::AdapterError>;

    /// Verify, based on the signature & state_root, that the signer is the same
    fn verify(
        &self,
        signer: &ValidatorId,
        state_root: &str,
        signature: &str,
    ) -> AdapterResult<bool, Self::AdapterError>;

    /// Validate a channel
    fn validate_channel<'a>(
        &'a self,
        channel: &'a Channel,
    ) -> BoxFuture<'a, AdapterResult<bool, Self::AdapterError>>;

    /// Get user session from token
    fn session_from_token<'a>(
        &'a self,
        token: &'a str,
    ) -> BoxFuture<'a, AdapterResult<Session, Self::AdapterError>>;

    /// Gets authentication for specific validator
    fn get_auth(&self, validator_id: &ValidatorId) -> AdapterResult<String, Self::AdapterError>;
}
