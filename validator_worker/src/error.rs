use primitives::adapter::AdapterErrorKind;
use primitives::ChannelId;
use std::fmt;

#[derive(Debug)]
pub enum TickError {
    TimedOut(tokio::time::Elapsed),
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

#[derive(Debug)]
pub enum Error<AE: AdapterErrorKind> {
    SentryApi(crate::sentry_interface::Error<AE>),
    LeaderTick(ChannelId, TickError),
    FollowerTick(ChannelId, TickError),
}

impl<AE: AdapterErrorKind> std::error::Error for Error<AE> {}

impl<AE: AdapterErrorKind> fmt::Display for Error<AE> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Error::*;

        match self {
            SentryApi(err) => write!(f, "SentryApi: {}", err),
            LeaderTick(channel_id, err) => write!(f, "LeaderTick {:#?}: {}", channel_id, err),
            FollowerTick(channel_id, err) => write!(f, "FollowerTick {:#?}: {}", channel_id, err),
        }
    }
}
