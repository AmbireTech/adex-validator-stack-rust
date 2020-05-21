use crate::{BigNum, Channel};
use std::collections::HashMap;

pub use eval::*;

mod eval;

#[derive(Debug, Clone)]
#[cfg_attr(test, derive(Default))]
pub struct Input {
    /// AdView scope, accessible only on the AdView
    pub ad_view: Option<AdView>,
    /// Global scope, accessible everywhere
    pub global: Global,
    /// adSlot scope, accessible on Supermarket and AdView
    pub ad_slot: Option<AdSlot>,
}

impl Input {
    fn try_get(&self, key: &str) -> Result<Value, Error> {
        match key {
            "adView.secondsSinceShow" => self
                .ad_view
                .as_ref()
                .map(|ad_view| Value::Number(ad_view.seconds_since_show.into()))
                .ok_or(Error::UnknownVariable),
            "adView.hasCustomPreferences" => self
                .ad_view
                .as_ref()
                .map(|ad_view| Value::Bool(ad_view.has_custom_preferences))
                .ok_or(Error::UnknownVariable),
            "adSlotId" => Ok(Value::String(self.global.ad_slot_id.clone())),
            "adUnitId" => Ok(Value::String(self.global.ad_unit_id.clone())),
            "adUnitType" => Ok(Value::String(self.global.ad_unit_type.clone())),
            "publisherId" => Ok(Value::String(self.global.publisher_id.clone())),
            "advertiserId" => Ok(Value::String(self.global.advertiser_id.clone())),
            "country" => Ok(Value::String(self.global.country.clone())),
            "eventType" => Ok(Value::String(self.global.event_type.clone())),
            "campaignId" => Ok(Value::String(self.global.campiagn_id.clone())),
            "campaignTotalSpent" => Ok(Value::String(self.global.campaign_total_spent.clone())),
            "campaignSecondsActive" => {
                Ok(Value::Number(self.global.campaign_seconds_active.into()))
            }
            "campaignSecondsDuration" => {
                Ok(Value::Number(self.global.campaign_seconds_duration.into()))
            }
            "campaignBudget" => Ok(Value::BigNum(self.global.campaign_budget.clone())),
            "eventMinPrice" => Ok(Value::BigNum(self.global.event_min_price.clone())),
            "eventMaxPrice" => Ok(Value::BigNum(self.global.event_max_price.clone())),
            "publisherEarnedFromCampaign" => Ok(Value::BigNum(
                self.global.publisher_earned_from_campaign.clone(),
            )),
            "secondsSinceEpoch" => Ok(Value::Number(self.global.seconds_since_epoch.into())),
            "userAgentOS" => Ok(Value::String(self.global.user_agent_os.clone())),
            "userAgentBrowserFamily" => {
                Ok(Value::String(self.global.user_agent_browser_family.clone()))
            }
            "adSlot.categories" => self
                .ad_slot
                .as_ref()
                .map(|ad_slot| {
                    let array = ad_slot
                        .categories
                        .iter()
                        .map(|string| Value::String(string.clone()))
                        .collect();
                    Value::Array(array)
                })
                .ok_or(Error::UnknownVariable),
            "adSlot.hostname" => self
                .ad_slot
                .as_ref()
                .map(|ad_slot| Value::String(ad_slot.hostname.clone()))
                .ok_or(Error::UnknownVariable),
            "adSlot.alexaRank" => {
                let ad_slot = self.ad_slot.as_ref().ok_or(Error::UnknownVariable)?;

                match serde_json::Number::from_f64(ad_slot.alexa_rank) {
                    Some(number) => Ok(Value::Number(number)),
                    None => Err(Error::TypeError),
                }
            }
            _unknown_field => Err(Error::UnknownVariable),
        }
    }
}

#[derive(Debug, Clone)]
#[cfg_attr(test, derive(Default))]
pub struct AdView {
    pub seconds_since_show: u64,
    pub has_custom_preferences: bool,
}

#[derive(Debug, Clone)]
#[cfg_attr(test, derive(Default))]
pub struct Global {
    pub ad_slot_id: String,
    pub ad_unit_id: String,
    pub ad_unit_type: String,
    pub publisher_id: String,
    pub advertiser_id: String,
    pub country: String,
    pub event_type: String,
    pub campiagn_id: String,
    pub campaign_total_spent: String,
    pub campaign_seconds_active: u64,
    pub campaign_seconds_duration: u64,
    pub campaign_budget: BigNum,
    pub event_min_price: BigNum,
    pub event_max_price: BigNum,
    pub publisher_earned_from_campaign: BigNum,
    pub seconds_since_epoch: u64,
    pub user_agent_os: String,
    pub user_agent_browser_family: String,
}

#[derive(Debug, Clone)]
#[cfg_attr(test, derive(Default))]
pub struct AdSlot {
    pub categories: Vec<String>,
    pub hostname: String,
    pub alexa_rank: f64,
}

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

impl From<&Channel> for Output {
    fn from(channel: &Channel) -> Self {
        let price = match &channel.spec.pricing_bounds {
            Some(pricing_bounds) => pricing_bounds
                .to_vec()
                .into_iter()
                .map(|(key, price)| (key.to_string(), price.min))
                .collect(),
            _ => Default::default(),
        };

        Self {
            show: true,
            boost: 1.0,
            price,
        }
    }
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

        let expected_number = serde_json::Number::from(10);

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
