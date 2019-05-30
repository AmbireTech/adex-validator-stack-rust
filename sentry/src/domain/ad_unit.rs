use chrono::{DateTime, Utc};
use chrono::serde::ts_milliseconds;
use serde::{Deserialize, Serialize};

use crate::domain::TargetingTag;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AdUnit {
    /// valid ipfs hash of spec props below
    ipfs: String,
    /// the type of the ad unit
    /// currently, possible values are:
    /// legacy_300x250, legacy_250x250, legacy_240x400, legacy_336x280,
    /// legacy_180x150, legacy_300x100, legacy_720x300, legacy_468x60,
    /// legacy_234x60, legacy_88x31, legacy_120x90, legacy_120x60,
    /// legacy_120x240, legacy_125x125, legacy_728x90, legacy_160x600,
    /// legacy_120x600, legacy_300x600
    /// see IAB ad unit guidelines and iab_flex_{adUnitName} (see IAB's new ad portfolio and PDF)
    #[serde(rename = "type")]
    ad_type: String,
    /// a URL to the resource (usually PNG); must use the ipfs:// protocol, to guarantee data immutability
    media_url: String,
    /// MIME type of the media, possible values at the moment are: image/jpeg, image/png
    media_mime: String,
    /// the advertised URL
    target_url: String,
    /// an array of TargetingTag, optional
    targeting: Vec<TargetingTag>,
    /// optional, Number; minimum targeting score
    min_targeting_score: Option<u8>,
    /// an array of TargetingTag, meant for discovery between publishers/advertisers
    tags: Vec<TargetingTag>,
    /// user address from the session
    owner: String,
    /// number, UTC timestamp in milliseconds, used as nonce for escaping duplicated spec ipfs hashes
    #[serde(with = "ts_milliseconds")]
    created: DateTime<Utc>,
    /// the name of the unit used in platform UI
    title: String,
    /// arbitrary text used in platform UI
    description: String,
    /// user can change it - used for filtering in platform UI
    archived: bool,
    /// UTC timestamp in milliseconds, changed every time modifiable property is changed
    #[serde(with = "ts_milliseconds")]
    modified: DateTime<Utc>,

}