#![deny(clippy::all)]

use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum ValidatorWokerError {
    ConfigurationError(String),
}
