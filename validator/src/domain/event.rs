use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use domain::{AdUnit, BigNum};

#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
#[derive(Serialize, Deserialize)]
pub enum Event {
    #[serde(rename_all = "camelCase")]
    Impression {
        publisher: String,
        ad_unit: AdUnit,
    },
    #[serde(rename_all = "camelCase")]
    UpdateImpressionPrice {
        price: BigNum,
    },
    #[serde(rename_all = "camelCase")]
    ImpressionWithCommission {
        earners: Vec<Earner>,
    },
    #[serde(rename_all = "camelCase")]
    Pay {
        outputs: HashMap<String, BigNum>,
    },
    PauseChannel,
    #[serde(skip_deserializing)]
    Close,
}

#[derive(Serialize, Deserialize)]
pub struct Earner {
    #[serde(rename = "publisher")]
    address: String,
    promilles: u64,
}
