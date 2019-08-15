use std::collections::HashMap;
use serde::{Deserialize, Serialize};

use crate::BigNum;

pub const DEVELOPMENT_CONFIG: &str = r#"
        max_channels = 512
        channels_find_limit = 200
        wait_time = 500

        aggr_throttle = 0
        events_find_limit = 100
        msgs_find_limit = 10

        heartbeat_time = 30000
        health_threshold_promilles = 950
        propagation_timeout = 1000

        list_timeout = 5000
        fetch_timeout = 5000
        validator_tick_timeout = 5000

        ip_rate_limit = { type = "ip", timeframe = 20000 }
        SID_RATE_LIMIT = { type = 'sid', timeframe = 20000 }

        ethereum_core_address = '0x333420fc6a897356e69b62417cd17ff012177d2b'
        ethereum_network = 'goerli'
    "#;

pub const PRODUCTION_CONFIG: &str = r#"
    # Maximum number of channels to return per request
    MAX_CHANNELS = 512
    
    channels_find_limit = 512
    wait_time = 500

    aggr_throttle = 5000
    events_find_limit = 100
    msgs_find_limit = 10

    heartbeat_time = 60000
    health_threshold_promilles = 970
    propagation_timeout = 3000

    list_timeout = 10000
    fetch_timeout = 10000
    validator_tick_timeout = 10000

    ip_rate_limit = { type = "ip", timeframe = 20000 }

    ethereum_core_address = '0x333420fc6a897356e69b62417cd17ff012177d2b'
    ethereum_network = 'homestead'
    token_address_whitelist = ['0x89d24A6b4CcB1B6fAA2625fE562bDD9a23260359']

    creators_whitelist = []
    minimal_deposit = 0
    minimal_fee = 0
    validators_whitelist = []
    "#;

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
    pub identity: String, // should not be here maybe?
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
    pub ip_rate_limit: Vec<RateLimit>, // HashMap??
    pub sid_rate_limt: Vec<RateLimit>, // HashMap ??
    pub creators_whitelist: Vec<String>,
    pub minimal_deposit: BigNum,
    pub minimal_fee: BigNum,
    pub token_address_whitelist: Vec<String>,
    pub ethereum_core_address: String,
    pub ethereum_network: String,
    pub validators_whitelist: Vec<String>,
}

// use primitives::Config;

// pub struct ConfigBuilder {
//     identity: String,
//     validators_whitelist: Vec<String>,
//     creators_whitelist: Vec<String>,
//     assets_whitelist: Vec<Asset>,
//     minimal_deposit: BigNum,
//     minimal_fee: BigNum,
// }

// impl ConfigBuilder {
//     pub fn new(identity: &str) -> Self {
//         Self {
//             identity: identity.to_string(),
//             validators_whitelist: Vec::new(),
//             creators_whitelist: Vec::new(),
//             assets_whitelist: Vec::new(),
//             minimal_deposit: 0.into(),
//             minimal_fee: 0.into(),
//         }
//     }

//     pub fn set_validators_whitelist(mut self, validators: &[&str]) -> Self {
//         self.validators_whitelist = validators.iter().map(|slice| slice.to_string()).collect();
//         self
//     }

//     pub fn set_creators_whitelist(mut self, creators: &[&str]) -> Self {
//         self.creators_whitelist = creators.iter().map(|slice| slice.to_string()).collect();
//         self
//     }

//     pub fn set_assets_whitelist(mut self, assets: &[Asset]) -> Self {
//         self.assets_whitelist = assets.to_vec();
//         self
//     }

//     pub fn set_minimum_deposit(mut self, minimum: BigNum) -> Self {
//         self.minimal_deposit = minimum;
//         self
//     }

//     pub fn set_minimum_fee(mut self, minimum: BigNum) -> Self {
//         self.minimal_fee = minimum;
//         self
//     }

//     pub fn build(self) -> Config {
//         Config {
//             identity: self.identity,
//             validators_whitelist: self.validators_whitelist,
//             creators_whitelist: self.creators_whitelist,
//             assets_whitelist: self.assets_whitelist,
//             minimal_deposit: self.minimal_deposit,
//             minimal_fee: self.minimal_fee,
//         }
//     }
// }
