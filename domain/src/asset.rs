use serde::{Deserialize, Serialize};

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

impl Into<String> for Asset {
    fn into(self) -> String {
        self.0
    }
}

#[cfg(any(test, feature = "fixtures"))]
pub(crate) mod fixtures {
    use fake::helper::take_one;

    use super::Asset;

    const ASSETS_LIST: [&str; 8] = ["DAI", "BGN", "EUR", "USD", "ADX", "BTC", "LIT", "ETH"];

    pub fn get_asset() -> Asset {
        take_one(&ASSETS_LIST).into()
    }
}
