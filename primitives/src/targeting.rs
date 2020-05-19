use crate::BigNum;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, convert::TryFrom};

pub use eval::*;

mod eval;

pub trait TryGet: Serialize {
    const PATTERN: &'static str = ".";

    fn try_get(&self, key: &str) -> Result<Value, Error> {
        let serde_value = serde_json::json!(self);
        let pointer = format!("/{pointer}", pointer = key.replace(Self::PATTERN, "/"));

        match serde_value.pointer(&pointer) {
            Some(serde_value) => Value::try_from(serde_value.clone()),
            None => Err(Error::UnknownVariable),
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

impl TryGet for Input {}
impl TryGet for Global {}
impl TryGet for AdView {}
impl TryGet for AdSlot {}

#[derive(Debug)]
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
        input.global.ad_slot_id = "ad_slot_id Value".to_string();
        input.global.campaign_budget = BigNum::from(50);
        input.ad_view = Some(AdView {
            seconds_since_show: 10,
            has_custom_preferences: false,
        });

        let ad_view_seconds_since_show = input
            .try_get("adView.secondsSinceShow")
            .expect("Should get the ad_view.seconds_since_show field");

        let expected_number =
            serde_json::from_str::<serde_json::Number>("10").expect("Should create number");

        assert_eq!(Value::Number(expected_number), ad_view_seconds_since_show);

        let ad_slot_id = input
            .try_get("adSlotId")
            .expect("Should get the global.ad_slot_id field");

        assert_eq!(Value::String("ad_slot_id Value".to_string()), ad_slot_id);

        let get_unknown = input
            .try_get("unknownField")
            .expect_err("Should return Error");

        assert_eq!(Error::UnknownVariable, get_unknown);

        let global_campaign_budget = input
            .try_get("campaignBudget")
            .expect("Should get the global.campaign_budget field");

        assert_eq!(Value::BigNum(BigNum::from(50)), global_campaign_budget);
    }
}
