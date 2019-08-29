#![deny(clippy::all)]

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ValidatorWorkerError {
    ConfigurationError(String),
    InvalidValidatorEntry(String)
}
