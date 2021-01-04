use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
pub struct EventSubmission {
    #[serde(default)]
    pub allow: Vec<Rule>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Rule {
    #[serde(default)]
    pub uids: Option<Vec<String>>,
    #[serde(default)]
    pub rate_limit: Option<RateLimit>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RateLimit {
    /// "ip", "uid"
    #[serde(rename = "type")]
    pub limit_type: String,
    /// in milliseconds
    #[serde(rename = "timeframe", with = "serde_millis")]
    pub time_frame: Duration,
}
