use primitives::adapter::AdapterErrorKind;
use primitives::ChannelId;
use std::fmt;
use thiserror::Error;

#[derive(Debug)]
pub enum TickError {
    TimedOut(tokio::time::error::Elapsed),
    Tick(Box<dyn std::error::Error>),
}

impl fmt::Display for TickError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TickError::TimedOut(err) => write!(f, "Tick TimedOut: ({})", err),
            TickError::Tick(err) => write!(f, "Tick: {}", err),
        }
    }
}

#[derive(Error, Debug)]
pub enum Error<AE: AdapterErrorKind + 'static> {
    #[error("SentryApi: {0}")]
    SentryApi(#[from] crate::sentry_interface::Error<AE>),
    #[error("LeaderTick {0}: {1}")]
    LeaderTick(ChannelId, TickError),
    #[error("FollowerTick {0}: {1}")]
    FollowerTick(ChannelId, TickError),
    #[error("Placeholder for Validation errors")]
    Validation,
    #[error("Placeholder for sum overflow error")]
    Overflow,
    #[error("Whoami is neither a Leader or follower in channel")]
    // TODO: Add channel, validatorId, etc.
    ChannelNotIntendedForUs
}