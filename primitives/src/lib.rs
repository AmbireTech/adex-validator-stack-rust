#![deny(rust_2018_idioms)]
#![deny(clippy::all)]
#![allow(deprecated)]
use std::{error, fmt};

pub use self::{
    ad_slot::AdSlot,
    ad_unit::AdUnit,
    address::Address,
    balances::Balances,
    balances_map::{BalancesMap, UnifiedMap},
    big_num::BigNum,
    campaign::{Campaign, CampaignId},
    channel::Channel,
    channel::ChannelId,
    config::Config,
    deposit::Deposit,
    event_submission::EventSubmission,
    ipfs::IPFS,
    unified_num::UnifiedNum,
    validator::{Validator, ValidatorDesc, ValidatorId},
};

mod ad_slot;
mod ad_unit;
pub mod adapter;
pub mod address;
pub mod analytics;
pub mod balances;
pub mod balances_map;
pub mod big_num;
pub mod campaign;
pub mod campaign_validator;
pub mod channel;
pub mod config;
mod eth_checksum;
pub mod event_submission;
pub mod ipfs;
pub mod market;
pub mod merkle_tree;
pub mod sentry;
pub mod spender;
pub mod supermarket;
pub mod targeting;
mod unified_num;
pub mod validator;

mod deposit {
    use crate::{BigNum, UnifiedNum};
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
    #[serde(rename_all = "camelCase")]
    pub struct Deposit<N> {
        pub total: N,
        pub still_on_create2: N,
    }

    impl Deposit<UnifiedNum> {
        pub fn to_precision(&self, precision: u8) -> Deposit<BigNum> {
            Deposit {
                total: self.total.to_precision(precision),
                still_on_create2: self.total.to_precision(precision),
            }
        }

        pub fn from_precision(
            deposit: Deposit<BigNum>,
            precision: u8,
        ) -> Option<Deposit<UnifiedNum>> {
            let total = UnifiedNum::from_precision(deposit.total, precision);
            let still_on_create2 = UnifiedNum::from_precision(deposit.still_on_create2, precision);

            match (total, still_on_create2) {
                (Some(total), Some(still_on_create2)) => Some(Deposit {
                    total,
                    still_on_create2,
                }),
                _ => None,
            }
        }
    }
}

pub mod util {
    pub use api::ApiUrl;

    pub mod api;
    pub mod tests {
        use slog::{o, Discard, Drain, Logger};

        pub mod prep_db;
        pub mod time;

        pub fn discard_logger() -> Logger {
            let drain = Discard.fuse();

            Logger::root(drain, o!())
        }
    }

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
