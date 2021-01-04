#![deny(rust_2018_idioms)]
#![deny(clippy::all)]
use std::error;
use std::fmt;

mod ad_slot;
mod ad_unit;
pub mod adapter;
pub mod balances_map;
pub mod big_num;
pub mod channel;
pub mod channel_validator;
pub mod config;
pub mod event_submission;
pub mod ipfs;
pub mod market;
pub mod merkle_tree;
pub mod sentry;
pub mod supermarket;
pub mod targeting;

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
pub mod analytics;
mod eth_checksum;
pub mod validator;

pub use self::ad_slot::AdSlot;
pub use self::ad_unit::AdUnit;
pub use self::balances_map::BalancesMap;
pub use self::big_num::BigNum;
pub use self::channel::{Channel, ChannelId, ChannelSpec, SpecValidator, SpecValidators};
pub use self::config::Config;
pub use self::event_submission::EventSubmission;
pub use self::ipfs::IPFS;
pub use self::validator::{ValidatorDesc, ValidatorId};

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

impl error::Error for DomainError {
    fn cause(&self) -> Option<&dyn error::Error> {
        None
    }
}

/// Trait that creates a String which is `0x` prefixed and encodes the bytes by `eth_checksum`
pub trait ToETHChecksum: AsRef<[u8]> {
    fn to_checksum(&self) -> String {
        // checksum replaces `0x` prefix and adds one itself
        eth_checksum::checksum(&hex::encode(self.as_ref()))
    }
}

impl ToETHChecksum for &[u8; 20] {}
