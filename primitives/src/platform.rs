pub use ad_unit::{AdUnitResponse, AdUnitsResponse};

mod ad_unit {
    use crate::AdUnit;
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
    #[serde(rename_all = "camelCase")]
    pub struct AdUnitsResponse(pub Vec<AdUnit>);

    #[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
    #[serde(rename_all = "camelCase")]
    pub struct AdUnitResponse {
        pub unit: AdUnit,
    }
}
