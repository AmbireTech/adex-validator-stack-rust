use crate::{
    chain::{Chain, ChainId},
    event_submission::RateLimit,
    util::ApiUrl,
    Address, BigNum, ChainOf, ValidatorId,
};
use once_cell::sync::Lazy;
use serde::{Deserialize, Deserializer, Serialize};
use std::{collections::HashMap, num::NonZeroU8, time::Duration};
use thiserror::Error;

pub use toml::de::Error as TomlError;

/// Production configuration found in `docs/config/prod.toml`
///
/// ```toml
#[doc = include_str!("../../docs/config/prod.toml")]
/// ```
pub static PRODUCTION_CONFIG: Lazy<Config> = Lazy::new(|| {
    toml::from_str(include_str!("../../docs/config/prod.toml"))
        .expect("Failed to parse prod.toml config file")
});

/// Ganache (dev) configuration found in `docs/config/ganache.toml`
///
/// ```toml
#[doc = include_str!("../../docs/config/ganache.toml")]
/// ```
pub static GANACHE_CONFIG: Lazy<Config> = Lazy::new(|| {
    Config::try_toml(include_str!("../../docs/config/ganache.toml"))
        .expect("Failed to parse ganache.toml config file")
});

/// The environment in which the application is running
/// Defaults to [`Environment::Development`]
#[derive(Debug, Deserialize, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "camelCase")]
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
pub struct Config {
    /// The maximum number of [`Channel`](crate::Channel)s that the worker
    /// can process for one tick.
    pub max_channels: u32,
    /// The maximum number of [`Channel`](crate::Channel)s per page
    /// returned by Sentry's GET `/v5/channel/list` route.
    ///
    /// Also see: [`ChannelListResponse`](crate::sentry::channel_list::ChannelListResponse)
    pub channels_find_limit: u32,
    /// The maximum number of [`Campaign`](crate::Campaign)s per page
    /// returned by Sentry's GET `/v5/campaign/list` route.
    ///
    /// Also see: [`CampaignListResponse`](crate::sentry::campaign_list::CampaignListResponse)
    pub campaigns_find_limit: u32,
    /// The maximum number of [`Spender`](crate::spender::Spender)s per page
    /// returned by Sentry's GET `/v5/channel/0xXXX.../spender/all` route.
    ///
    /// Also see: [`AllSpendersResponse`](crate::sentry::AllSpendersResponse)
    pub spendable_find_limit: u32,
    /// The Validator Worker tick time.
    ///
    /// The [`Channel`](crate::Channel)s' tick and the wait time should both
    /// finish before running a new tick in the Validator Worker.
    ///
    /// In milliseconds
    #[serde(deserialize_with = "milliseconds_to_std_duration")]
    pub wait_time: Duration,
    /// The maximum allowed limit of [`ValidatorMessage`](crate::sentry::ValidatorMessage)s per page
    /// returned by Sentry's GET `/v5/channel/0xXXX.../validator-messages` route.
    ///
    /// Request query also has a `limit` parameter, which can be used to return
    /// <= `msgs_find_limit` messages in the request.
    ///
    /// Also see: [`ValidatorMessagesListResponse`](crate::sentry::ValidatorMessagesListResponse),
    /// [`ValidatorMessagesListQuery`](crate::sentry::ValidatorMessagesListQuery)
    pub msgs_find_limit: u32,
    /// The maximum allowed limit of [`FetchedAnalytics`](crate::sentry::FetchedAnalytics)s per page
    /// returned by Sentry's GET `/v5/analytics` routes:
    ///
    /// - GET `/v5/analytics`
    /// - GET `/v5/analytics/for-publisher`
    /// - GET `/v5/analytics/for-advertiser`
    /// - GET `/v5/analytics/for-admin`
    ///
    /// Request query also has a `limit` parameter, which can be used to return
    /// <= `analytics_find_limit` messages in the request.
    ///
    /// Also see: [`AnalyticsQuery`](crate::analytics::AnalyticsQuery)
    pub analytics_find_limit: u32,
    /// A timeout to be used when collecting the Analytics for a requests:
    /// - GET `/v5/analytics`
    /// - GET `/v5/analytics/for-publisher`
    /// - GET `/v5/analytics/for-advertiser`
    /// - GET `/v5/analytics/for-admin`
    ///
    /// In milliseconds
    #[serde(deserialize_with = "milliseconds_to_std_duration")]
    pub analytics_maxtime: Duration,
    /// The amount of time that should have passed before sending a new heartbeat.
    ///
    /// In milliseconds
    #[serde(deserialize_with = "milliseconds_to_std_duration")]
    pub heartbeat_time: Duration,
    /// The pro miles below which the [`ApproveState`](crate::validator::ApproveState)
    /// becomes **unhealthy** in the [`Channel`](crate::Channel)'s Follower.
    ///
    /// Also see: [`ApproveState.is_healthy`](crate::validator::ApproveState::is_healthy)
    ///
    /// In pro milles (<= 1000)
    pub health_threshold_promilles: u32,
    /// The pro milles below which the [`ApproveState`](crate::validator::ApproveState)
    /// will not be triggered and instead a [`RejectState`](crate::validator::RejectState)
    /// will be propagated by the [`Channel`](crate::Channel)'s Follower.
    ///
    /// In pro milles (<= 1000)
    pub health_unsignable_promilles: u32,
    /// Sets the timeout for propagating a Validator message ([`MessageTypes`](crate::validator::MessageTypes))
    /// to a validator.
    ///
    /// In milliseconds
    #[serde(deserialize_with = "milliseconds_to_std_duration")]
    pub propagation_timeout: Duration,
    /// The Client timeout for `SentryApi`.
    ///
    /// This includes all requests made to sentry except propagating messages.
    /// When propagating messages we make requests to foreign Sentry
    /// instances and we use a separate timeout -
    /// [`Config.propagation_timeout`](Config::propagation_timeout).
    ///
    /// In milliseconds
    #[serde(deserialize_with = "milliseconds_to_std_duration")]
    pub fetch_timeout: Duration,
    /// The Client timeout for `SentryApi` when collecting all channels
    /// and Validators using the `/campaign/list` route.
    ///
    /// In milliseconds
    #[serde(deserialize_with = "milliseconds_to_std_duration")]
    pub all_campaigns_timeout: Duration,
    /// The timeout for a single tick of a [`Channel`](crate::Channel) in
    /// the Validator Worker.
    /// This timeout is applied to both the leader and follower ticks.
    ///
    /// In milliseconds
    #[serde(deserialize_with = "milliseconds_to_std_duration")]
    pub channel_tick_timeout: Duration,
    /// The default IP rate limit that will be imposed if
    /// [`Campaign.event_submission`](crate::Campaign::event_submission) is [`None`].
    pub ip_rate_limit: RateLimit,
    /// An optional whitelisted addresses for [`Campaign.creator`](crate::Campaign::creator)s.
    ///
    /// If empty, any address will be allowed to create a [`Campaign`](crate::Campaign).
    pub creators_whitelist: Vec<Address>,
    /// An optional whitelisted Validator addresses for [`Campaign.validators`](crate::Campaign::validators).
    ///
    /// If empty, any address will be allowed to be a validator in a [`Campaign`](crate::Campaign).
    pub validators_whitelist: Vec<ValidatorId>,
    pub admins: Vec<Address>,
    /// The key of this map is a human-readable text of the Chain name
    /// for readability in the configuration file.
    ///
    /// - To get the chain of a token address use [`Config::find_chain_of()`].
    ///
    /// - To get a [`ChainInfo`] only by a [`ChainId`] use [`Config::find_chain()`].
    ///
    /// **NOTE:** Make sure that a Token [`Address`] is unique across all Chains,
    /// otherwise [`Config::find_chain_of()`] will fetch only one of them and cause unexpected problems.
    #[serde(rename = "chain")]
    pub chains: HashMap<String, ChainInfo>,
    pub platform: PlatformConfig,
    pub limits: Limits,
}

impl Config {
    /// Utility method that will deserialize a Toml file content into a [`Config`].
    ///
    /// Rather than relying on the `toml` crate directly, use this method instead.
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
    pub fn find_chain_of(&self, token: Address) -> Option<ChainOf<()>> {
        self.chains.values().find_map(|chain_info| {
            chain_info
                .tokens
                .values()
                .find(|token_info| token_info.address == token)
                .map(|token_info| ChainOf::new(chain_info.chain.clone(), token_info.clone()))
        })
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct PlatformConfig {
    pub url: ApiUrl,
    #[serde(deserialize_with = "milliseconds_to_std_duration")]
    pub keep_alive_interval: Duration,
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
    pub min_campaign_budget: BigNum,
    pub min_validator_fee: BigNum,
    pub precision: NonZeroU8,
    pub address: Address,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Limits {
    pub units_for_slot: limits::UnitsForSlot,
}

fn milliseconds_to_std_duration<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: Deserializer<'de>,
{
    use serde::de::Error;
    use toml::Value;

    let toml_value: Value = Value::deserialize(deserializer)?;

    let milliseconds = match toml_value {
        Value::Integer(mills) => u64::try_from(mills).map_err(Error::custom),
        _ => Err(Error::custom("Only integers allowed for this value")),
    }?;

    Ok(Duration::from_millis(milliseconds))
}

pub mod limits {
    use serde::{Deserialize, Serialize};

    use crate::UnifiedNum;

    /// Limits applied to the `POST /units-for-slot` route
    #[derive(Serialize, Deserialize, Debug, Clone)]
    pub struct UnitsForSlot {
        /// The maximum number of campaigns a publisher can earn from.
        /// This will limit the returned Campaigns to the set number.
        #[serde(default)]
        pub max_campaigns_earning_from: u16,
        /// If the resulting targeting price is lower than this value,
        /// it will filter out the given AdUnit.
        pub global_min_impression_price: UnifiedNum,
    }
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
    use super::{GANACHE_CONFIG, PRODUCTION_CONFIG};

    /// Makes sure that both config files are correct and won't be left in a
    /// broken state.
    #[test]
    fn correct_config_files() {
        let _ganache = GANACHE_CONFIG.clone();
        let _production = PRODUCTION_CONFIG.clone();
    }
}
