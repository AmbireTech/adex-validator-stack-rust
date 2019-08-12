use std::pin::Pin;
use futures::{Future, FutureExt};
use std::collections::HashMap;
// use domain::validator::message::State;
use crate::{ Channel, Config };
//
//use crate::sanity::SanityChecker;
use std::error::Error;
use std::fmt;

pub type AdapterFuture<T> = Pin<Box<dyn Future<Output = Result<T, AdapterError>> + Send>>;

#[derive(Debug, Eq, PartialEq)]
pub enum AdapterError {
    Authentication(String),
}

impl Error for AdapterError {}

impl fmt::Display for AdapterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AdapterError::Authentication(error) => write!(f, "Authentication error: {}", error),
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

pub trait Adapter {
    /// Initialize adapter
    fn init(self, opts: &AdapterOptions, config: &Config) -> Self;

    /// Unlock adapter
    fn unlock(&self) -> AdapterFuture<bool>;

    /// Get Adapter whoami
    fn whoami(&self) -> String;

    /// Signs the provided state_root
    fn sign(
        &self,
        state_root: String,
    ) -> AdapterFuture<String>;

    /// Verify, based on the signature & state_root, that the signer is the same
    fn verify(
        &self,
        signer: &str,
        state_root: &str,
        signature: &str,
    ) -> AdapterFuture<bool>;

    /// Validate a channel
    fn validate_channel(&self, channel: &Channel) -> AdapterFuture<bool>;

    /// Get user session from token
    fn session_from_token(&self, token: &str) ->  AdapterFuture<String>;

    /// Gets authentication for specific validator
    fn get_auth(&self, validator: &str) -> AdapterFuture<String>;

    //   @TODO
    // fn get_balance_leaf()

    fn signable_state_root(
        channel_id: &str,
        balance_root: &str,
    ) -> String;
}

//
//pub struct ChannelId(pub [u8; 32]);
//impl AsRef<[u8]> for ChannelId {
//    fn as_ref(&self) -> &[u8] {
//        &self.0
//    }
//}
//
//pub struct BalanceRoot(pub [u8; 32]);
//impl AsRef<[u8]> for BalanceRoot {
//    fn as_ref(&self) -> &[u8] {
//        &self.0
//    }
//}
//
//pub struct SignableStateRoot<T: fmt::Display>(pub T);
//