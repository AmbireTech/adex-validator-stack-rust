use crate::event_submission::RateLimit;
use crate::{BigNum, ValidatorId};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use serde_hex::{SerHex, StrictPfx};
use std::fs;

lazy_static! {
    static ref DEVELOPMENT_CONFIG: Config =
        toml::from_str(include_str!("../../docs/config/dev.toml"))
            .expect("Failed to parse dev.toml config file");
    static ref PRODUCTION_CONFIG: Config =
        toml::from_str(include_str!("../../docs/config/prod.toml"))
            .expect("Failed to parse prod.toml config file");
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
    pub token_address_whitelist: Vec<String>,
    #[serde(with = "SerHex::<StrictPfx>")]
    pub ethereum_core_address: [u8; 20],
    pub ethereum_network: String,
    pub ethereum_adapter_relayer: String,
    pub validators_whitelist: Vec<ValidatorId>,
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
