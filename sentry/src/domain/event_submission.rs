use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EventSubmission {
    #[serde(default)]
    allow: Vec<Rule>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Rule {
    #[serde(default)]
    uids: Option<Vec<String>>,
    #[serde(default)]
    rate_limit: Opiton<RateLimit>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RateLimit {
    /// "ip", "uid"
    limit_type: String,
    /// in milliseconds
    #[serde(remame = "timeframe")]
    time_frame: u64,
}