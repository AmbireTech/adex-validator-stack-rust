use crate::channel_validator::ChannelValidator;
use crate::{Channel, DomainError, ValidatorId};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::From;
use std::error::Error;
use std::fmt;
use std::fmt::Debug;

pub type AdapterResult<T> = Result<T, AdapterError>;

#[derive(Debug, Eq, PartialEq)]
pub enum AdapterError {
    Authentication(String),
    EwtVerifyFailed(String),
    Authorization(String),
    Configuration(String),
    Signature(String),
    InvalidChannel(String),
    Failed(String),
}

impl Error for AdapterError {}

impl fmt::Display for AdapterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AdapterError::Authentication(error) => write!(f, "Authentication error: {}", error),
            AdapterError::EwtVerifyFailed(error) => write!(f, "Ewt verification error: {}", error),
            AdapterError::Authorization(error) => write!(f, "Authorization error: {}", error),
            AdapterError::Configuration(error) => write!(f, "Configuration error: {}", error),
            AdapterError::Signature(error) => write!(f, "Signature error: {}", error),
            AdapterError::InvalidChannel(error) => write!(f, "Invalid Channel error: {}", error),
            AdapterError::Failed(error) => write!(f, "error: {}", error),
        }
    }
}

impl From<DomainError> for AdapterError {
    fn from(err: DomainError) -> AdapterError {
        AdapterError::Failed(err.to_string())
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

pub trait Adapter: ChannelValidator + Send + Clone + Debug {
    /// Unlock adapter
    fn unlock(&mut self) -> AdapterResult<()>;

    /// Get Adapter whoami
    fn whoami(&self) -> &ValidatorId;

    /// Signs the provided state_root
    fn sign(&self, state_root: &str) -> AdapterResult<String>;

    /// Verify, based on the signature & state_root, that the signer is the same
    fn verify(
        &self,
        signer: &ValidatorId,
        state_root: &str,
        signature: &str,
    ) -> AdapterResult<bool>;

    /// Validate a channel
    fn validate_channel(&self, channel: &Channel) -> AdapterResult<bool>;

    /// Get user session from token
    fn session_from_token(&self, token: &str) -> AdapterResult<Session>;

    /// Gets authentication for specific validator
    fn get_auth(&mut self, validator_id: &ValidatorId) -> AdapterResult<String>;
}
