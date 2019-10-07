use serde::{Deserialize, Serialize};
use std::error::Error;
use std::fmt::{Display, Formatter, Result};

#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
pub enum ValidatorWorker {
    Configuration(String),
    Failed(String),
}

impl Error for ValidatorWorker {}

impl Display for ValidatorWorker {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self {
            ValidatorWorker::Configuration(error) => write!(f, "Configuration error: {}", error),
            ValidatorWorker::Failed(error) => write!(f, "error: {}", error),
        }
    }
}
