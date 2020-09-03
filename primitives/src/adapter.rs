use crate::channel::ChannelError;
use crate::channel_validator::ChannelValidator;
use crate::{Channel, DomainError, ValidatorId};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::convert::From;
use std::fmt;

pub type AdapterResult<T, AE> = Result<T, Error<AE>>;

pub trait AdapterErrorKind: fmt::Debug + fmt::Display {}

#[derive(Debug)]
pub enum Error<AE: AdapterErrorKind> {
    Authentication(String),
    Authorization(String),
    InvalidChannel(ChannelError),
    /// Adapter specific errors
    // Since we don't know the size of the Adapter Error we use a Box to limit the size of this enum
    Adapter(Box<AE>),
    Domain(DomainError),
    /// You need to `.unlock()` the wallet first
    LockedWallet,
}

impl<AE: AdapterErrorKind> std::error::Error for Error<AE> {}

impl<AE: AdapterErrorKind> From<AE> for Error<AE> {
    fn from(adapter_err: AE) -> Self {
        Self::Adapter(Box::new(adapter_err))
    }
}

impl<AE: AdapterErrorKind> fmt::Display for Error<AE> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Authentication(error) => write!(f, "Authentication: {}", error),
            Error::Authorization(error) => write!(f, "Authorization: {}", error),
            Error::InvalidChannel(error) => write!(f, "{}", error),
            Error::Adapter(error) => write!(f, "Adapter: {}", *error),
            Error::Domain(error) => write!(f, "Domain: {}", error),
            Error::LockedWallet => write!(f, "You must `.unlock()` the wallet first"),
        }
    }
}

impl<AE: AdapterErrorKind> From<DomainError> for Error<AE> {
    fn from(err: DomainError) -> Error<AE> {
        Error::Domain(err)
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

#[async_trait]
pub trait Adapter: ChannelValidator + Send + Sync + fmt::Debug + Clone {
    type AdapterError: AdapterErrorKind + 'static;

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
    async fn validate_channel<'a>(
        &'a self,
        channel: &'a Channel,
    ) -> AdapterResult<bool, Self::AdapterError>;

    /// Get user session from token
    async fn session_from_token<'a>(
        &'a self,
        token: &'a str,
    ) -> AdapterResult<Session, Self::AdapterError>;

    /// Gets authentication for specific validator
    fn get_auth(&self, validator_id: &ValidatorId) -> AdapterResult<String, Self::AdapterError>;
}
