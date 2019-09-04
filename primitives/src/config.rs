use crate::BigNum;
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use std::fs;
use toml;

lazy_static! {
    static ref DEVELOPMENT_CONFIG: Config =
        toml::from_str(include_str!("../../docs/config/dev.toml")).unwrap();
    static ref PRODUCTION_CONFIG: Config =
        toml::from_str(include_str!("../../docs/config/prod.toml")).unwrap();
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RateLimit {
    /// "ip", "uid"
    #[serde(rename = "type")]
    pub limit_type: String,
    /// in milliseconds
    #[serde(rename = "timeframe")]
    pub time_frame: u64,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Config {
    pub identity: Option<String>, // should not be here maybe?
    pub max_channels: u32,
    pub wait_time: u32,
    pub aggr_throttle: u32,
    pub heartbeat_time: u32,
    pub channels_find_limit: u32,
    pub events_find_limit: u32,
    pub health_threshold_promilles: u32,
    pub propagation_timeout: u32,
    pub fetch_timeout: u32,
    pub list_timeout: u32,
    pub validator_tick_timeout: u32,
    pub ip_rate_limit: RateLimit,  // HashMap??
    pub sid_rate_limit: RateLimit, // HashMap ??
    pub creators_whitelist: Vec<String>,
    pub minimal_deposit: BigNum,
    pub minimal_fee: BigNum,
    pub token_address_whitelist: Vec<String>,
    pub ethereum_core_address: String,
    pub ethereum_network: String,
    pub validators_whitelist: Vec<String>,
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
