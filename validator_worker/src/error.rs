use primitives::ChannelId;
use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt::{Display, Formatter, Result};

#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
pub enum ValidatorWorker {
    Configuration(String),
    Failed(String),
    Channel(ChannelId, String),
}

impl Error for ValidatorWorker {}

impl Display for ValidatorWorker {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self {
            ValidatorWorker::Configuration(err) => write!(f, "Configuration error: {}", err),
            ValidatorWorker::Failed(err) => write!(f, "error: {}", err),
            ValidatorWorker::Channel(channel_id, err) => {
                write!(f, "Channel {}: {}", channel_id, err)
            }
        }
    }
}
