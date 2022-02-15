use crate::{
    chain::{Chain, ChainId},
    event_submission::RateLimit,
    Address, BigNum, ChainOf, ValidatorId,
};
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, num::NonZeroU8};
use thiserror::Error;

pub use toml::de::Error as TomlError;

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
    /// The default development setup is running `ganache-cli` locally.
    Development,
    Production,
}

impl Default for Environment {
    fn default() -> Self {
        Self::Development
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all(serialize = "SCREAMING_SNAKE_CASE"))]
pub struct Config {
    /// Maximum number of channels to return per request
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
    pub ip_rate_limit: RateLimit,
    pub sid_rate_limit: RateLimit,
    pub creators_whitelist: Vec<Address>,
    pub validators_whitelist: Vec<ValidatorId>,
    pub admins: Vec<String>,
    /// The key of this map is a human-readable text of the Chain name
    /// for readability in the configuration file.
    ///
    /// - To get the chain of a token address use [`Config::find_token_chain`].
    ///
    /// - To get a chain RPC use [`Config::find_chain_rpc`].
    ///
    /// **NOTE:** Make sure that a Token [`Address`] is unique across all Chains,
    /// otherwise `Config::find_chain_token` will fetch only one of them and cause unexpected problems.
    #[serde(rename = "chain")]
    pub chains: HashMap<String, ChainInfo>,
}

impl Config {
    /// Utility method that will deserialize a Toml file content into a [`Config`].
    ///
    /// Instead of relying on the `toml` crate directly, use this method instead.
    pub fn try_toml(toml: &str) -> Result<Self, TomlError> {
        toml::from_str(toml)
    }

    /// Finds a [`Chain`] based on the [`ChainId`].
    pub fn find_chain(&self, chain_id: ChainId) -> Option<&ChainInfo> {
        self.chains
            .values()
            .find(|chain_info| chain_info.chain.chain_id == chain_id)
    }

    /// Finds the pair of Chain & Token, given only a token [`Address`].
    pub fn find_chain_token(&self, token: Address) -> Option<ChainOf<()>> {
        self.chains.values().find_map(|chain_info| {
            chain_info
                .tokens
                .values()
                .find(|token_info| token_info.address == token)
                .map(|token_info| ChainOf::new(chain_info.chain.clone(), token_info.clone()))
        })
    }
}

/// Configured chain with tokens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChainInfo {
    #[serde(flatten)]
    pub chain: Chain,
    /// A Chain should always have whitelisted tokens configured,
    /// otherwise there is no reason to have the chain set up.
    #[serde(rename = "token")]
    pub tokens: HashMap<String, TokenInfo>,
}

impl ChainInfo {
    pub fn find_token(&self, token: Address) -> Option<&TokenInfo> {
        self.tokens
            .values()
            .find(|token_info| token_info.address == token)
    }
}

/// Configured Token in a specific [`Chain`].
/// Precision can differ for the same token from one [`Chain`] to another.
#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq, Hash)]
pub struct TokenInfo {
    pub min_token_units_for_deposit: BigNum,
    pub min_validator_fee: BigNum,
    pub precision: NonZeroU8,
    pub address: Address,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Toml parsing: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("File reading: {0}")]
    InvalidFile(#[from] std::io::Error),
}

/// If no `config_file` path is provided it will load the [`Environment`] configuration.
/// If `config_file` path is provided it will try to read and parse the file in Toml format.
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
            Environment::Development => Ok(GANACHE_CONFIG.clone()),
        },
    }
}

#[cfg(test)]
mod test {
    use std::collections::HashMap;

    use serde::Deserialize;

    use crate::{Address, BigNum};

    #[derive(Debug, Deserialize)]
    pub struct TokenConf {
        pub whitelisted_token: HashMap<String, Token>,
    }

    impl TokenConf {
        pub fn find_chain(&self, token_address: Address) -> Option<&TokenChain> {
            self.whitelisted_token
                .values()
                .find_map(|token| token.find_chain(token_address))
        }
    }

    #[derive(Debug, Deserialize)]
    pub struct Token {
        pub min_token_units_for_deposit: BigNum,
        pub min_validator_fee: BigNum,
        #[serde(rename = "chain")]
        pub chains: HashMap<String, TokenChain>,
    }

    impl Token {
        pub fn find_chain(&self, token: Address) -> Option<&TokenChain> {
            self.chains
                .values()
                .find(|token_chain| token_chain.address == token)
        }
    }

    #[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
    pub struct TokenChain {
        pub chain_id: u64,
        pub address: Address,
    }

    #[test]
    fn whitelisted_tokens_toml() {
        let toml = include_str!("test_conf.toml");

        let _token_config =
            toml::from_str::<toml::Value>(toml).expect("Should deserialize into toml::Value");

        let token_config =
            toml::from_str::<TokenConf>(toml).expect("Should deserialize into TokenConf");

        // find Chain id by the USDT token Address for Ethereum Mainnet from USDT key only
        {
            let usdt_eth_mainnet: Address = "0x509ee0d083ddf8ac028f2a56731412edd63223b9"
                .parse()
                .expect("Should be valid address");
            let found_token = token_config.whitelisted_token["USDT"]
                .find_chain(usdt_eth_mainnet)
                .expect("Should find the Chain Id for USDT mainnet by the address");

            let expected = TokenChain {
                chain_id: 1,
                address: usdt_eth_mainnet,
            };
            assert_eq!(&expected, found_token);
        }

        // find Chain id by the DAI token Address for Ethereum Ropsten from all whitelisted tokens
        {
            let dai_eth_ropsten: Address = "0x65600c50Ea42e225368Ded6c3789a539284aA62C"
                .parse()
                .expect("Should be valid address");
            let found_token = token_config
                .find_chain(dai_eth_ropsten)
                .expect("Should find the Chain Id for USDT mainnet by the address");

            let expected = TokenChain {
                chain_id: 3,
                address: dai_eth_ropsten,
            };
            assert_eq!(&expected, found_token);
        }
    }
}
