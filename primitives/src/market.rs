use chrono::serde::ts_milliseconds;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use url::Url;

use std::fmt;

use crate::{AdSlot, AdUnit, BalancesMap, BigNum, Channel};
pub use ad_unit::AdUnitsResponse;

// Data structs specific to the market
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum StatusType {
    Active,
    Ready,
    Pending,
    Initializing,
    Waiting,
    Offline,
    Disconnected,
    Unhealthy,
    Invalid,
    Expired,
    /// also called "Closed"
    Exhausted,
    Withdraw,
}

impl fmt::Display for StatusType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    #[serde(rename = "name")]
    pub status_type: StatusType,
    pub usd_estimate: Option<f32>,
    #[serde(rename = "lastApprovedBalances")]
    pub balances: BalancesMap,
    #[serde(with = "ts_milliseconds")]
    pub last_checked: DateTime<Utc>,
}

impl Status {
    pub fn balances_sum(&self) -> BigNum {
        self.balances.values().sum()
    }
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Campaign {
    #[serde(flatten)]
    pub channel: Channel,
    pub status: Status,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    rename_all = "camelCase",
    from = "ad_slot::ShimResponse",
    into = "ad_slot::ShimResponse"
)]
pub struct AdSlotResponse {
    pub slot: AdSlot,
    pub accepted_referrers: Vec<Url>,
    pub categories: Vec<String>,
    pub alexa_rank: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(
    rename_all = "camelCase",
    from = "ad_unit::ShimAdUnitResponse",
    into = "ad_unit::ShimAdUnitResponse"
)]
pub struct AdUnitResponse {
    pub unit: AdUnit,
}

mod ad_unit {
    use crate::{AdUnit, ValidatorId, IPFS};
    use chrono::{DateTime, Utc};
    use serde::{Deserialize, Serialize};

    use super::AdUnitResponse;

    #[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
    #[serde(rename_all = "camelCase", from = "Vec<Shim>", into = "Vec<Shim>")]
    pub struct AdUnitsResponse(pub Vec<AdUnit>);

    impl From<Vec<Shim>> for AdUnitsResponse {
        fn from(vec_of_shims: Vec<Shim>) -> Self {
            Self(vec_of_shims.into_iter().map(Into::into).collect())
        }
    }

    impl Into<Vec<Shim>> for AdUnitsResponse {
        fn into(self) -> Vec<Shim> {
            self.0.into_iter().map(Into::into).collect()
        }
    }

    #[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
    #[serde(rename_all = "camelCase")]
    pub struct ShimAdUnitResponse {
        unit: Shim,
    }

    impl From<AdUnitResponse> for ShimAdUnitResponse {
        fn from(response: AdUnitResponse) -> Self {
            Self {
                unit: response.unit.into(),
            }
        }
    }

    impl From<ShimAdUnitResponse> for AdUnitResponse {
        fn from(shim: ShimAdUnitResponse) -> Self {
            Self {
                unit: shim.unit.into(),
            }
        }
    }

    #[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
    #[serde(rename_all = "camelCase")]
    /// This AdUnit Shim has only one difference with the Validator [`AdUnit`](crate::AdUnit)
    /// The `created` and `modified` timestamps here are in strings (see https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Date/Date#Timestamp_string),
    /// instead of being millisecond timestamps
    pub struct Shim {
        pub ipfs: IPFS,
        #[serde(rename = "type")]
        pub ad_type: String,
        pub media_url: String,
        pub media_mime: String,
        pub target_url: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub min_targeting_score: Option<f64>,
        pub owner: ValidatorId,
        pub created: DateTime<Utc>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub title: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub description: Option<String>,
        #[serde(default)]
        pub archived: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub modified: Option<DateTime<Utc>>,
    }

    impl Into<AdUnit> for Shim {
        fn into(self) -> AdUnit {
            AdUnit {
                ipfs: self.ipfs,
                ad_type: self.ad_type,
                media_url: self.media_url,
                media_mime: self.media_mime,
                target_url: self.target_url,
                min_targeting_score: self.min_targeting_score,
                owner: self.owner,
                created: self.created,
                title: self.title,
                description: self.description,
                archived: self.archived,
                modified: self.modified,
            }
        }
    }

    impl From<AdUnit> for Shim {
        fn from(ad_unit: AdUnit) -> Self {
            Self {
                ipfs: ad_unit.ipfs,
                ad_type: ad_unit.ad_type,
                media_url: ad_unit.media_url,
                media_mime: ad_unit.media_mime,
                target_url: ad_unit.target_url,
                min_targeting_score: ad_unit.min_targeting_score,
                owner: ad_unit.owner,
                created: ad_unit.created,
                title: ad_unit.title,
                description: ad_unit.description,
                archived: ad_unit.archived,
                modified: ad_unit.modified,
            }
        }
    }
}

mod ad_slot {
    use std::collections::HashMap;

    use chrono::{DateTime, Utc};
    use serde::{Deserialize, Serialize};
    use url::Url;

    use crate::{targeting::Rule, AdSlot, BigNum, ValidatorId};

    #[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
    #[serde(rename_all = "camelCase")]
    pub struct ShimResponse {
        pub slot: Shim,
        pub accepted_referrers: Vec<Url>,
        pub categories: Vec<String>,
        pub alexa_rank: Option<f64>,
    }

    impl From<super::AdSlotResponse> for ShimResponse {
        fn from(response: super::AdSlotResponse) -> Self {
            Self {
                slot: Shim::from(response.slot),
                accepted_referrers: response.accepted_referrers,
                categories: response.categories,
                alexa_rank: response.alexa_rank,
            }
        }
    }

    impl From<ShimResponse> for super::AdSlotResponse {
        fn from(shim_response: ShimResponse) -> Self {
            Self {
                slot: shim_response.slot.into(),
                accepted_referrers: shim_response.accepted_referrers,
                categories: shim_response.categories,
                alexa_rank: shim_response.alexa_rank,
            }
        }
    }

    #[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
    #[serde(rename_all = "camelCase")]
    /// This AdSlot Shim has only one difference with the Validator `primitives::AdSlot`
    /// The `created` and `modified` timestamps here are in strings (see https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Date/Date#Timestamp_string),
    /// instead of being millisecond timestamps
    pub struct Shim {
        pub ipfs: String,
        #[serde(rename = "type")]
        pub ad_type: String,
        #[serde(default)]
        pub min_per_impression: Option<HashMap<String, BigNum>>,
        #[serde(default)]
        pub rules: Vec<Rule>,
        #[serde(default)]
        pub fallback_unit: Option<String>,
        pub owner: ValidatorId,
        /// DateTime uses `RFC 3339` by default
        /// This is not the usual `milliseconds timestamp`
        /// as the original
        pub created: DateTime<Utc>,
        #[serde(default)]
        pub title: Option<String>,
        #[serde(default)]
        pub description: Option<String>,
        #[serde(default)]
        pub website: Option<String>,
        #[serde(default)]
        pub archived: bool,
        /// DateTime uses `RFC 3339` by default
        /// This is not the usual `milliseconds timestamp`
        /// as the original
        pub modified: Option<DateTime<Utc>>,
    }

    impl From<AdSlot> for Shim {
        fn from(ad_slot: AdSlot) -> Self {
            Self {
                ipfs: ad_slot.ipfs,
                ad_type: ad_slot.ad_type,
                min_per_impression: ad_slot.min_per_impression,
                rules: ad_slot.rules,
                fallback_unit: ad_slot.fallback_unit,
                owner: ad_slot.owner,
                created: ad_slot.created,
                title: ad_slot.title,
                description: ad_slot.description,
                website: ad_slot.website,
                archived: ad_slot.archived,
                modified: ad_slot.modified,
            }
        }
    }

    impl Into<AdSlot> for Shim {
        fn into(self) -> AdSlot {
            AdSlot {
                ipfs: self.ipfs,
                ad_type: self.ad_type,
                min_per_impression: self.min_per_impression,
                rules: self.rules,
                fallback_unit: self.fallback_unit,
                owner: self.owner,
                created: self.created,
                title: self.title,
                description: self.description,
                website: self.website,
                archived: self.archived,
                modified: self.modified,
            }
        }
    }
}
