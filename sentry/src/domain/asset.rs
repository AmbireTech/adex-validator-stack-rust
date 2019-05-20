use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Asset(pub(crate) String);

impl From<String> for Asset {
    fn from(asset: String) -> Self {
        Self(asset)
    }
}

impl From<&str> for Asset {
    fn from(asset: &str) -> Self {
        Self(asset.to_string())
    }
}
