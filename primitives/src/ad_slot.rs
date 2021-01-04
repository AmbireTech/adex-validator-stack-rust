use crate::{targeting::Rule, BigNum, ValidatorId};
use chrono::{
    serde::{ts_milliseconds, ts_milliseconds_option},
    DateTime, Utc,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// See [AdEx Protocol adSlot.md][protocol] & [adex-models AdSlot.js][adex-models] for more details.
/// [protocol]: https://github.com/AdExNetwork/adex-protocol/blob/master/adSlot.md
/// [adex-models]: https://github.com/AdExNetwork/adex-models/blob/master/src/models/AdSlot.js
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AdSlot {
    /// valid ipfs hash of spec props below
    pub ipfs: String,
    /// The type of the AdSlot
    /// currently, possible values are:
    /// > legacy_300x250, legacy_250x250, legacy_240x400, legacy_336x280,
    /// > legacy_180x150, legacy_300x100, legacy_720x300, legacy_468x60,
    /// > legacy_234x60, legacy_88x31, legacy_120x90, legacy_120x60,
    /// > legacy_120x240, legacy_125x125, legacy_728x90, legacy_160x600,
    /// > legacy_120x600, legacy_300x600
    /// see IAB ad unit guidelines and iab_flex_{adUnitName} (see IAB's new ad portfolio and PDF)
    #[serde(rename = "type")]
    pub ad_type: String,
    // HashMap<DepositAsset, BigNum> for the minimum payment accepted per impression
    #[serde(default)]
    pub min_per_impression: Option<HashMap<String, BigNum>>,
    #[serde(default)]
    pub rules: Vec<Rule>,
    /// Valid ipfs hash for Ad Unit object. It will be used as fallback data (optional)
    #[serde(default)]
    pub fallback_unit: Option<String>,
    /// User address from the session
    pub owner: ValidatorId,
    /// UTC timestamp in milliseconds, used as nonce for escaping duplicated spec ipfs hashes
    #[serde(with = "ts_milliseconds")]
    pub created: DateTime<Utc>,
    /// the name of the unit used in platform UI
    #[serde(default)]
    pub title: Option<String>,
    /// arbitrary text used in platform UI
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub website: Option<String>,
    /// user can change it - used for filtering in platform UI
    #[serde(default)]
    pub archived: bool,
    /// UTC timestamp in milliseconds, changed every time modifiable property is changed
    #[serde(with = "ts_milliseconds_option")]
    pub modified: Option<DateTime<Utc>>,
}
