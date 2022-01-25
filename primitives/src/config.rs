use crate::{event_submission::RateLimit, Address, BigNum, ValidatorId};
use once_cell::sync::Lazy;
use serde::{Deserialize, Deserializer, Serialize};
use serde_hex::{SerHex, StrictPfx};
use std::{collections::HashMap, num::NonZeroU8};
use thiserror::Error;

pub use toml::de::Error as TomlError;

pub static DEVELOPMENT_CONFIG: Lazy<Config> = Lazy::new(|| {
    toml::from_str(include_str!("../../docs/config/dev.toml"))
        .expect("Failed to parse dev.toml config file")
});

pub static PRODUCTION_CONFIG: Lazy<Config> = Lazy::new(|| {
    toml::from_str(include_str!("../../docs/config/prod.toml"))
        .expect("Failed to parse prod.toml config file")
});

pub static GANACHE_CONFIG: Lazy<Config> = Lazy::new(|| {
    Config::try_toml(include_str!("../../docs/config/ganache.toml"))
        .expect("Failed to parse ganache.toml config file")
});

#[derive(Debug, Deserialize, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "camelCase")]
/// The environment in which the application is running
/// Defaults to [`Environment::Development`]
pub enum Environment {
    Development,
    Production,
}

impl Default for Environment {
    fn default() -> Self {
        Self::Development
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TokenInfo {
    pub min_token_units_for_deposit: BigNum,
    pub min_validator_fee: BigNum,
    pub precision: NonZeroU8,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all(serialize = "SCREAMING_SNAKE_CASE"))]
pub struct Config {
    pub max_channels: u32,
    pub channels_find_limit: u32,
    pub campaigns_find_limit: u32,
    pub spendable_find_limit: u32,
    pub wait_time: u32,
    pub msgs_find_limit: u32,
    pub analytics_find_limit_v5: u32,
    /// In milliseconds
    pub analytics_maxtime_v5: u32,
    /// In milliseconds
    pub heartbeat_time: u32,
    pub health_threshold_promilles: u32,
    pub health_unsignable_promilles: u32,
    /// Sets the timeout for propagating a Validator message to a validator
    /// In Milliseconds
    pub propagation_timeout: u32,
    /// in milliseconds
    /// Set's the Client timeout for `SentryApi`
    /// This includes all requests made to sentry except propagating messages.
    /// When propagating messages we make requests to foreign Sentry instances as well.
    pub fetch_timeout: u32,
    /// In Milliseconds
    pub all_campaigns_timeout: u32,
    /// In Milliseconds
    pub channel_tick_timeout: u32,
    pub ip_rate_limit: RateLimit,  // HashMap??
    pub sid_rate_limit: RateLimit, // HashMap ??
    #[serde(with = "SerHex::<StrictPfx>")]
    pub outpace_address: [u8; 20],
    #[serde(with = "SerHex::<StrictPfx>")]
    pub sweeper_address: [u8; 20],
    pub ethereum_network: String,
    pub creators_whitelist: Vec<Address>,
    pub validators_whitelist: Vec<ValidatorId>,
    pub admins: Vec<String>,
    #[serde(deserialize_with = "deserialize_token_whitelist")]
    pub token_address_whitelist: HashMap<Address, TokenInfo>,
}

impl Config {
    /// Utility method that will deserialize a Toml file content into a `Config`.
    ///
    /// Instead of relying on the `toml` crate directly, use this method instead.
    pub fn try_toml(toml: &str) -> Result<Self, TomlError> {
        toml::from_str(toml)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct ConfigWhitelist {
    address: Address,
    min_token_units_for_deposit: BigNum,
    min_validator_fee: BigNum,
    precision: NonZeroU8,
}

fn deserialize_token_whitelist<'de, D>(
    deserializer: D,
) -> Result<HashMap<Address, TokenInfo>, D::Error>
where
    D: Deserializer<'de>,
{
    let array: Vec<ConfigWhitelist> = Deserialize::deserialize(deserializer)?;

    let tokens_whitelist: HashMap<Address, TokenInfo> = array
        .into_iter()
        .map(|config_whitelist| {
            (
                config_whitelist.address,
                TokenInfo {
                    min_token_units_for_deposit: config_whitelist.min_token_units_for_deposit,
                    min_validator_fee: config_whitelist.min_validator_fee,
                    precision: config_whitelist.precision,
                },
            )
        })
        .collect();

    Ok(tokens_whitelist)
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Toml parsing: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("File reading: {0}")]
    InvalidFile(#[from] std::io::Error),
}

pub fn configuration(
    environment: Environment,
    config_file: Option<&str>,
) -> Result<Config, ConfigError> {
    match config_file {
        Some(config_file) => {
            let content = std::fs::read(config_file)?;

            Ok(toml::from_slice(&content)?)
        }
        None => match environment {
            Environment::Production => Ok(PRODUCTION_CONFIG.clone()),
            Environment::Development => Ok(DEVELOPMENT_CONFIG.clone()),
        },
    }
}
