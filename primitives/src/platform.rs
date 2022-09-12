use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use url::Url;

use crate::{AdSlot, AdUnit};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Website {
    #[serde(default)]
    pub categories: HashSet<String>,
    #[serde(default)]
    pub accepted_referrers: Vec<Url>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdSlotResponse {
    pub slot: AdSlot,
    /// The fetched Fallback [`AdUnit`] ( [`AdSlot.fallback_unit`] ) of the [`AdSlot`] if set.
    ///
    /// [`AdSlot.fallback_unit`]: AdSlot::fallback_unit
    #[serde(default)]
    pub fallback: Option<AdUnit>,
    /// The [`AdSlot.website`] information if it's provided and information is available for it.
    ///
    /// [`AdSlot.website`]: AdSlot::website
    #[serde(default, flatten)]
    pub website: Option<Website>,
}
