use std::pin::Pin;
use futures::prelude::*;
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

    type Output;

    /// Initialize adapter
    fn init(opts: AdapterOptions, config: &Config) -> Self::Output;

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
}