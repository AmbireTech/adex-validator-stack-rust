use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use domain::{AdUnit, BigNum};

#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
#[derive(Serialize, Deserialize)]
pub enum Event {
    Impression {
        publisher: String,
    },
    #[serde(rename_all = "camelCase")]
    UpdateImpressionPrice {
        publisher: String,
        price: u64,
        ad_unit: AdUnit,
    },
    ImpressionWithCommission {
        earner: Vec<Earner>,
    },
    ImpressionPricePerCase {},
    Pay {
        outputs: HashMap<String, BigNum>,
    },
    PauseChannel {},
    #[serde(skip_deserializing)]
    Close {},
}

#[derive(Serialize, Deserialize)]
pub struct Earner {
    publisher: String,
    promilles: u64,
}
