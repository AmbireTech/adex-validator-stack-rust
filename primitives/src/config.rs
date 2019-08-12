use std::collections::HashMap;

use crate::BigNum;

#[derive(Debug, Clone)]
pub struct Config {
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
    pub validator_tick: u32,
    pub ip_rate_limit: Vec<String>, // change
    pub sid_rate_limt: Vec<String>,
    pub creators_whitelist: Vec<String>,
    pub minimal_deposit: BigNum,
    pub minimal_fee: BigNum,
    pub token_address_whitelist: Vec<String>,
    pub ethereum_core_addr: String,
    pub ethereum_network: String,
    pub validators_whitelist: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            validators_whitelist: vec![],
            creators_whitelist: vec![],
        }
    }
}