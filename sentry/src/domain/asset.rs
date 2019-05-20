use serde::{Serialize, Deserialize};
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Asset(String);

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

impl Into<String> for &Asset {
    fn into(self) -> String {
        self.0.clone()
    }
}