use chrono::{
    serde::{ts_milliseconds, ts_milliseconds_option},
    DateTime, Utc,
};
use serde::{Deserialize, Serialize};

use crate::{ValidatorId, IPFS};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct AdUnit {
    /// valid ipfs hash of spec props below
    pub ipfs: IPFS,
    /// the type of the ad unit
    /// currently, possible values are:
    /// legacy_300x250, legacy_250x250, legacy_240x400, legacy_336x280,
    /// legacy_180x150, legacy_300x100, legacy_720x300, legacy_468x60,
    /// legacy_234x60, legacy_88x31, legacy_120x90, legacy_120x60,
    /// legacy_120x240, legacy_125x125, legacy_728x90, legacy_160x600,
    /// legacy_120x600, legacy_300x600
    /// see IAB ad unit guidelines and iab_flex_{adUnitName} (see IAB's new ad portfolio and PDF)
    #[serde(rename = "type")]
    pub ad_type: String,
    /// a URL to the resource (usually PNG); must use the ipfs:// protocol, to guarantee data immutability
    pub media_url: String,
    /// MIME type of the media, possible values at the moment are: image/jpeg, image/png
    pub media_mime: String,
    /// Advertised URL
    pub target_url: String,
    /// Number; minimum targeting score (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_targeting_score: Option<f64>,
    /// user address from the session
    pub owner: ValidatorId,
    /// number, UTC timestamp in milliseconds, used as nonce for escaping duplicated spec ipfs hashes
    #[serde(with = "ts_milliseconds")]
    pub created: DateTime<Utc>,
    /// the name of the unit used in platform UI
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// arbitrary text used in platform UI
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// user can change it - used for filtering in platform UI
    #[serde(default)]
    pub archived: bool,
    /// UTC timestamp in milliseconds, changed every time modifiable property is changed
    #[serde(
        default,
        with = "ts_milliseconds_option",
        skip_serializing_if = "Option::is_none"
    )]
    pub modified: Option<DateTime<Utc>>,
}
