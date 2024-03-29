#![deny(rust_2018_idioms)]
#![deny(clippy::all)]
#![deny(rustdoc::broken_intra_doc_links)]
#![cfg_attr(docsrs, feature(doc_cfg))]
// TODO: Remove once stabled and upstream num::Integer::div_floor(...) is fixed
#![allow(unstable_name_collisions)]
use std::{error, fmt};

#[doc(inline)]
pub use self::{
    ad_slot::AdSlot,
    ad_unit::AdUnit,
    address::Address,
    balances::Balances,
    balances_map::{BalancesMap, UnifiedMap},
    big_num::BigNum,
    campaign::{Campaign, CampaignId},
    chain::{Chain, ChainId, ChainOf},
    channel::{Channel, ChannelId},
    config::Config,
    deposit::Deposit,
    event_submission::EventSubmission,
    ipfs::IPFS,
    unified_num::UnifiedNum,
    validator::{Validator, ValidatorDesc, ValidatorId},
};

mod ad_slot;
mod ad_unit;
pub mod address;
pub mod analytics;
pub mod balances;
pub mod balances_map;
pub mod big_num;
pub mod campaign;
pub mod campaign_validator;
mod chain;
pub mod channel;
pub mod config;
mod eth_checksum;
pub mod event_submission;
pub mod ipfs;
pub mod merkle_tree;
pub mod platform;
pub mod sentry;
pub mod spender;
pub mod targeting;
// It's not possible to enable this feature for doctest,
// so we always must pass `--feature=test-util` or `--all-features` when running doctests:
// `cargo test --doc --all-features`
//
// See issue: <https://github.com/rust-lang/rust/issues/67295>
#[cfg(any(test, feature = "test-util"))]
#[cfg_attr(docsrs, doc(cfg(feature = "test-util")))]
pub mod test_util;
pub mod unified_num;
pub mod validator;

/// This module is available with the `postgres` feature.
///
/// Other places where you'd find `mod postgres` implementations is for many of the structs in the crate
/// all of which implement [`tokio_postgres::types::FromSql`], [`tokio_postgres::types::ToSql`] or [`From<&tokio_postgres::Row>`]
#[cfg(feature = "postgres")]
#[cfg_attr(docsrs, doc(cfg(feature = "postgres")))]
pub mod postgres {
    use std::env::{self, VarError};

    use deadpool_postgres::{Manager, ManagerConfig, Pool, RecyclingMethod};
    use once_cell::sync::Lazy;
    use tokio_postgres::{Config, NoTls};

    pub type DbPool = deadpool_postgres::Pool;

    /// A Postgres pool with reasonable settings:
    /// - [`RecyclingMethod::Verified`]
    /// - `Pool::max_size = 32`
    /// Created using environment variables, see [`POSTGRES_CONFIG`].
    pub static POSTGRES_POOL: Lazy<Pool> = Lazy::new(|| {
        let config = POSTGRES_CONFIG.clone();

        let mgr_config = ManagerConfig {
            recycling_method: RecyclingMethod::Verified,
        };
        let mgr = Manager::from_config(config, NoTls, mgr_config);

        Pool::builder(mgr)
            .max_size(42)
            .build()
            .expect("Should build test postgres pool")
    });

    /// `POSTGRES_USER` environment variable - default: `postgres`
    pub static POSTGRES_USER: Lazy<String> =
        Lazy::new(|| env::var("POSTGRES_USER").unwrap_or_else(|_| String::from("postgres")));

    /// `POSTGRES_PASSWORD` environment variable - default: `postgres`
    pub static POSTGRES_PASSWORD: Lazy<String> =
        Lazy::new(|| env::var("POSTGRES_PASSWORD").unwrap_or_else(|_| String::from("postgres")));

    /// `POSTGRES_HOST` environment variable - default: `localhost`
    pub static POSTGRES_HOST: Lazy<String> =
        Lazy::new(|| env::var("POSTGRES_HOST").unwrap_or_else(|_| String::from("localhost")));

    /// `POSTGRES_PORT` environment variable - default: `5432`
    pub static POSTGRES_PORT: Lazy<u16> = Lazy::new(|| {
        env::var("POSTGRES_PORT")
            .unwrap_or_else(|_| String::from("5432"))
            .parse()
            .unwrap()
    });

    /// `POSTGRES_DB` environment variable - default: `POSTGRES_USER`
    pub static POSTGRES_DB: Lazy<String> = Lazy::new(|| match env::var("POSTGRES_DB") {
        Ok(database) => database,
        Err(VarError::NotPresent) => POSTGRES_USER.clone(),
        Err(err) => panic!("{}", err),
    });

    /// Postgres configuration derived from the environment variables:
    /// - POSTGRES_USER
    /// - POSTGRES_PASSWORD
    /// - POSTGRES_HOST
    /// - POSTGRES_PORT
    /// - POSTGRES_DB
    pub static POSTGRES_CONFIG: Lazy<Config> = Lazy::new(|| {
        let mut config = Config::new();

        config
            .user(POSTGRES_USER.as_str())
            .password(POSTGRES_PASSWORD.as_str())
            .host(POSTGRES_HOST.as_str())
            .port(*POSTGRES_PORT)
            .dbname(POSTGRES_DB.as_ref());

        config
    });
}

mod deposit {
    use crate::{BigNum, UnifiedNum};
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    #[serde(rename_all = "camelCase")]
    pub struct Deposit<N> {
        pub total: N,
    }

    impl Deposit<UnifiedNum> {
        pub fn to_precision(&self, precision: u8) -> Deposit<BigNum> {
            Deposit {
                total: self.total.to_precision(precision),
            }
        }

        pub fn from_precision(
            deposit: Deposit<BigNum>,
            precision: u8,
        ) -> Option<Deposit<UnifiedNum>> {
            let total = UnifiedNum::from_precision(deposit.total, precision);

            total.map(|total| Deposit { total })
        }
    }

    impl<N: Default> Default for Deposit<N> {
        fn default() -> Self {
            Self {
                total: Default::default(),
            }
        }
    }
}

pub mod util {
    #[doc(inline)]
    pub use api::ApiUrl;

    pub mod api;

    pub mod logging;
}

#[derive(Debug, PartialEq, Eq)]
pub enum DomainError {
    InvalidArgument(String),
    RuleViolation(String),
}

impl fmt::Display for DomainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DomainError::InvalidArgument(err) => write!(f, "{}", err),
            DomainError::RuleViolation(err) => write!(f, "{}", err),
        }
    }
}

impl From<address::Error> for DomainError {
    fn from(error: address::Error) -> Self {
        Self::InvalidArgument(error.to_string())
    }
}

impl error::Error for DomainError {
    fn cause(&self) -> Option<&dyn error::Error> {
        None
    }
}

/// Trait that creates a String which is `0x` prefixed and encodes the bytes by `eth_checksum`
#[allow(clippy::upper_case_acronyms)]
pub trait ToETHChecksum: AsRef<[u8]> {
    fn to_checksum(&self) -> String {
        // checksum replaces `0x` prefix and adds one itself
        eth_checksum::checksum(&hex::encode(self.as_ref()))
    }
}

impl ToETHChecksum for &[u8; 20] {}

pub trait ToHex {
    // Hex encoded `String`, **without** __Checksum__ming the string
    fn to_hex(&self) -> String;

    // Hex encoded `0x` prefixed `String`, **without** __Checksum__ming the string
    fn to_hex_prefixed(&self) -> String;
}

impl<T: AsRef<[u8]>> ToHex for T {
    fn to_hex(&self) -> String {
        hex::encode(self.as_ref())
    }

    fn to_hex_prefixed(&self) -> String {
        format!("0x{}", self.as_ref().to_hex())
    }
}
