use crate::{event_submission::RateLimit, Address, BigNum, ValidatorId};
use once_cell::sync::Lazy;
use serde::{Deserialize, Deserializer, Serialize};
use serde_hex::{SerHex, StrictPfx};
use std::{collections::HashMap, fs, num::NonZeroU8};

static DEVELOPMENT_CONFIG: Lazy<Config> = Lazy::new(|| {
    toml::from_str(include_str!("../../docs/config/dev.toml"))
        .expect("Failed to parse dev.toml config file")
});
static PRODUCTION_CONFIG: Lazy<Config> = Lazy::new(|| {
    toml::from_str(include_str!("../../docs/config/prod.toml"))
        .expect("Failed to parse prod.toml config file")
});

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
    pub wait_time: u32,
    #[deprecated = "redundant V4 value. No aggregates are needed for V5"]
    pub aggr_throttle: u32,
    #[deprecated = "For V5 this should probably be part of the Analytics"]
    pub events_find_limit: u32,
    pub msgs_find_limit: u32,
    pub analytics_find_limit_v5: u32,
    // in milliseconds
    pub analytics_maxtime_v5: u32,
    // in milliseconds
    pub heartbeat_time: u32,
    pub health_threshold_promilles: u32,
    pub health_unsignable_promilles: u32,
    /// in milliseconds
    /// set's the Client timeout for [`SentryApi`]
    /// This includes requests made for propagating new messages
    pub fetch_timeout: u32,
    /// in milliseconds
    pub validator_tick_timeout: u32,
    pub ip_rate_limit: RateLimit,  // HashMap??
    pub sid_rate_limit: RateLimit, // HashMap ??
    #[serde(with = "SerHex::<StrictPfx>")]
    pub outpace_address: [u8; 20],
    #[serde(with = "SerHex::<StrictPfx>")]
    pub sweeper_address: [u8; 20],
    pub ethereum_network: String,
    pub ethereum_adapter_relayer: String,
    pub creators_whitelist: Vec<Address>,
    pub validators_whitelist: Vec<ValidatorId>,
    #[serde(deserialize_with = "deserialize_token_whitelist")]
    pub token_address_whitelist: HashMap<Address, TokenInfo>,
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

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ConfigError {
    InvalidFile(String),
}

pub fn configuration(environment: &str, config_file: Option<&str>) -> Result<Config, ConfigError> {
    match config_file {
        Some(config_file) => match fs::read_to_string(config_file) {
            Ok(config) => match toml::from_str(&config) {
                Ok(data) => data,
                Err(e) => Err(ConfigError::InvalidFile(e.to_string())),
            },
            Err(e) => Err(ConfigError::InvalidFile(format!(
                "Unable to read provided config file {} {}",
                config_file, e
            ))),
        },
        None => match environment {
            "production" => Ok(PRODUCTION_CONFIG.clone()),
            _ => Ok(DEVELOPMENT_CONFIG.clone()),
        },
    }
}
