#![deny(rust_2018_idioms)]
#![deny(clippy::all)]
use std::error;
use std::fmt;

pub mod ad_unit;
pub mod adapter;
pub mod balances_map;
pub mod big_num;
pub mod channel;
pub mod channel_validator;
pub mod config;
pub mod event_submission;
pub mod market_channel;
pub mod merkle_tree;
pub mod sentry;
pub mod targeting_tag;
pub mod util {
    pub mod tests {
        pub mod prep_db;
        pub mod time;
    }

    pub mod logging;
}
pub mod validator;

pub use self::ad_unit::AdUnit;
pub use self::balances_map::BalancesMap;
pub use self::big_num::BigNum;
pub use self::channel::{Channel, ChannelId, ChannelSpec, SpecValidator, SpecValidators};
pub use self::config::Config;
pub use self::event_submission::EventSubmission;
pub use self::targeting_tag::TargetingTag;
pub use self::validator::{ValidatorDesc, ValidatorId};

#[derive(Debug, PartialEq, Eq)]
pub enum DomainError {
    InvalidArgument(String),
    RuleViolation(String),
}

impl fmt::Display for DomainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Domain error",)
    }
}

impl error::Error for DomainError {
    fn cause(&self) -> Option<&dyn error::Error> {
        None
    }
}
