use crate::BigNum;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, convert::TryFrom};

pub use eval::*;

mod eval;

pub trait TryGet {
    const PATTERN: &'static str;

    fn try_get(&self, key: &str) -> Result<Value, Error>;
}

impl<T: Serialize> TryGet for T {
    const PATTERN: &'static str = ".";

    fn try_get(&self, key: &str) -> Result<Value, Error> {
        let mut splitn = key.splitn(2, '.');

        let (field, remaining_key) = (splitn.next(), splitn.next());
        let serde_value = serde_json::json!(self);

        // filter empty string
        let field = field.filter(|s| !s.is_empty());
        let remaining_key = remaining_key.filter(|s| !s.is_empty());

        // TODO: Check what type of error we should return in each case
        match (field, remaining_key, serde_value) {
            (Some(field), remaining_key, serde_json::Value::Object(map)) => {
                match map.get(field) {
                    Some(serde_value) => serde_value.try_get(remaining_key.unwrap_or_default()),
                    None => Err(Error::TypeError),
                }
            },
            (None, None, serde_value) => Value::try_from(serde_value.clone()),
            // if we have any other field or remaining_key, it's iligal:
            // i.e. `first.second` for values that are not map
            _ => Err(Error::UnknownVariable),
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(test, derive(Default))]
#[serde(rename_all = "camelCase")]
pub struct Input {
    /// AdView scope, accessible only on the AdView
    #[serde(default)]
    pub ad_view: Option<AdView>,
    /// Global scope, accessible everywhere
    #[serde(flatten)]
    pub global: Global,
    /// adSlot scope, accessible on Supermarket and AdView
    #[serde(default)]
    pub ad_slot: Option<AdSlot>,
}

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(test, derive(Default))]
#[serde(rename_all = "camelCase")]
pub struct AdView {
    pub seconds_since_show: u64,
    pub has_custom_preferences: bool,
}

#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(test, derive(Default))]
#[serde(rename_all = "camelCase")]
pub struct Global {
    pub ad_slot_id: String,
    pub ad_unit_id: String,
    pub ad_unit_type: String,
    pub publisher_id: String,
    pub advertiser_id: String,
    pub country: String,
    pub event_type: String,
    pub campaign_total_spent: String,
    pub campaign_seconds_active: u64,
    pub campaign_seconds_duration: u64,
    pub campaign_budget: BigNum,
    pub event_min_price: BigNum,
    pub event_max_price: BigNum,
    pub publisher_earned_from_campaign: BigNum,
    pub seconds_since_epoch: u64,
    #[serde(rename = "userAgentOS")]
    pub user_agent_os: String,
    pub user_agent_browser_family: String,
}


#[derive(Debug, Serialize, Deserialize)]
#[cfg_attr(test, derive(Default))]
#[serde(rename_all = "camelCase")]
pub struct AdSlot {
    pub categories: Vec<String>,
    pub hostname: String,
    pub alexa_rank: f64,
}

pub struct Output {
    /// Whether to show the ad
    /// Default: true
    pub show: bool,
    /// The boost is a number between 0 and 5 that increases the likelyhood for the ad
    /// to be chosen if there is random selection applied on the AdView (multiple ad candidates with the same price)
    /// Default: 1.0
    pub boost: f64,
    /// price.{eventType}
    /// For example: price.IMPRESSION
    /// The default is the min of the bound of event type:
    /// Default: pricingBounds.IMPRESSION.min
    pub price: HashMap<String, BigNum>,
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_try_get_of_input() {
        let mut input = Input::default();
        input.ad_view = Some(AdView {
            seconds_since_show: 10, has_custom_preferences: false
        });

        let result = input.try_get("adView.secondsSinceShow").expect("Should get the adView field");

        let expected = serde_json::from_str::<serde_json::Number>("10").expect("Should create number");

        assert_eq!(Value::Number(expected), result)
    }
}
