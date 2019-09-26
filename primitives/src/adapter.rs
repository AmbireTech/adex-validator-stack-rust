use crate::channel_validator::ChannelValidator;
use crate::validator::ValidatorDesc;
use crate::{Channel, Config};
use std::collections::HashMap;
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
    IO(String),
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
            AdapterError::IO(error) => write!(f, "IO error: {}", error),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AdapterOptions {
    pub dummy_identity: Option<String>,
    pub dummy_auth: Option<HashMap<String, String>>,
    pub dummy_auth_tokens: Option<HashMap<String, String>>,
    pub keystore_file: Option<String>,
    pub keystore_pwd: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Session {
    pub era: i64,
    pub uid: String,
}

pub trait Adapter: ChannelValidator + Clone + Debug {
    type Output;

    /// Initialize adapter
    fn init(opts: AdapterOptions, config: &Config) -> AdapterResult<Self::Output>;

    /// Unlock adapter
    fn unlock(&self) -> AdapterResult<bool>;

    /// Get Adapter whoami
    fn whoami(&self) -> AdapterResult<String>;

    /// Signs the provided state_root
    fn sign(&self, state_root: &str) -> AdapterResult<String>;

    /// Verify, based on the signature & state_root, that the signer is the same
    fn verify(&self, signer: &str, state_root: &str, signature: &str) -> AdapterResult<bool>;

    /// Validate a channel
    fn validate_channel(&self, channel: &Channel) -> AdapterResult<bool>;

    /// Get user session from token
    fn session_from_token(&self, token: &str) -> AdapterResult<Session>;

    /// Gets authentication for specific validator
    fn get_auth(&self, validator: &ValidatorDesc) -> AdapterResult<String>;
}
