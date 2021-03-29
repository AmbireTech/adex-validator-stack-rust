use crate::event_submission::RateLimit;
use crate::{Address, BigNum, ValidatorId};
use lazy_static::lazy_static;
use serde::{Deserialize, Deserializer, Serialize};
use serde_hex::{SerHex, StrictPfx};
use std::collections::HashMap;
use std::fs;
use std::num::NonZeroU8;

lazy_static! {
    static ref DEVELOPMENT_CONFIG: Config =
        toml::from_str(include_str!("../../docs/config/dev.toml"))
            .expect("Failed to parse dev.toml config file");
    static ref PRODUCTION_CONFIG: Config =
        toml::from_str(include_str!("../../docs/config/prod.toml"))
            .expect("Failed to parse prod.toml config file");
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TokenInfo {
    pub min_token_units_for_deposit: BigNum,
    pub precision: NonZeroU8,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all(serialize = "SCREAMING_SNAKE_CASE"))]
pub struct Config {
    pub max_channels: u32,
    pub wait_time: u32,
    pub aggr_throttle: u32,
    pub heartbeat_time: u32, // in milliseconds
    pub channels_find_limit: u32,
    pub events_find_limit: u32,
    pub msgs_find_limit: u32,
    pub health_threshold_promilles: u32,
    pub health_unsignable_promilles: u32,
    pub propagation_timeout: u32,
    pub fetch_timeout: u32,
    pub validator_tick_timeout: u32,
    pub ip_rate_limit: RateLimit,  // HashMap??
    pub sid_rate_limit: RateLimit, // HashMap ??
    pub creators_whitelist: Vec<ValidatorId>,
    pub minimal_deposit: BigNum,
    pub minimal_fee: BigNum,
    #[serde(deserialize_with = "deserialize_token_whitelist")]
    pub token_address_whitelist: HashMap<Address, TokenInfo>,
    #[serde(with = "SerHex::<StrictPfx>")]
    pub ethereum_core_address: [u8; 20],
    pub ethereum_network: String,
    pub ethereum_adapter_relayer: String,
    pub validators_whitelist: Vec<ValidatorId>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct ConfigWhitelist {
    address: Address,
    min_token_units_for_deposit: BigNum,
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
        .map(|i| {
            (
                i.address,
                TokenInfo {
                    min_token_units_for_deposit: i.min_token_units_for_deposit,
                    precision: i.precision,
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
