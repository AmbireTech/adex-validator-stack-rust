use crate::{
    channel::Pricing, supermarket::Status, AdUnit, BalancesMap, BigNum, Channel, ToETHChecksum,
    ValidatorId,
};
use chrono::Utc;
use std::collections::HashMap;

pub use eval::*;
use serde_json::Number;

mod eval;

#[derive(Debug, Clone)]
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
        let spec = &self.global.channel.spec;

        match key {
            "adView.secondsSinceCampaignImpression" => self
                .ad_view
                .as_ref()
                .map(|ad_view| Value::Number(ad_view.seconds_since_campaign_impression.into()))
                .ok_or(Error::UnknownVariable),
            "adView.hasCustomPreferences" => self
                .ad_view
                .as_ref()
                .map(|ad_view| Value::Bool(ad_view.has_custom_preferences))
                .ok_or(Error::UnknownVariable),
            "adSlotId" => Ok(Value::String(self.global.ad_slot_id.clone())),
            "adSlotType" => Ok(Value::String(self.global.ad_slot_type.clone())),
            "publisherId" => Ok(Value::String(self.global.publisher_id.to_checksum())),
            "secondsSinceEpoch" => Ok(Value::Number(self.global.seconds_since_epoch.into())),
            "userAgentOS" => self
                .global
                .user_agent_os
                .clone()
                .map(Value::String)
                .ok_or(Error::UnknownVariable),
            "userAgentBrowserFamily" => self
                .global
                .user_agent_browser_family
                .clone()
                .map(Value::String)
                .ok_or(Error::UnknownVariable),

            "adUnitId" => {
                let ipfs = self
                    .global
                    .ad_unit
                    .as_ref()
                    .map(|ad_unit| ad_unit.ipfs.clone());
                Ok(Value::String(ipfs.unwrap_or_default()))
            }
            "advertiserId" => {
                let creator = self.global.channel.creator.to_hex_prefix_string();

                Ok(Value::String(creator))
            }
            "campaignId" => Ok(Value::String(self.global.channel.id.to_string())),
            "campaignTotalSpent" => Ok(Value::BigNum(
                self.global
                    .balances
                    .as_ref()
                    .map(|b| b.values().sum())
                    .unwrap_or_default(),
            )),
            "campaignSecondsActive" => {
                let duration = Utc::now() - spec.active_from.unwrap_or(spec.created);

                let seconds = duration
                    .to_std()
                    .map(|duration| duration.as_secs())
                    .unwrap_or(0);

                Ok(Value::Number(seconds.into()))
            }
            "campaignSecondsDuration" => {
                let duration =
                    spec.withdraw_period_start - spec.active_from.unwrap_or(spec.created);
                let seconds = duration
                    .to_std()
                    .map(|std_duration| std_duration.as_secs())
                    .unwrap_or(0);

                Ok(Value::Number(seconds.into()))
            }
            "campaignBudget" => Ok(Value::BigNum(self.global.channel.deposit_amount.clone())),
            "eventMinPrice" => {
                let min = get_pricing_bounds(&self.global.channel, &self.global.event_type).min;
                Ok(Value::BigNum(min))
            }
            "eventMaxPrice" => {
                let max = get_pricing_bounds(&self.global.channel, &self.global.event_type).max;
                Ok(Value::BigNum(max))
            }
            "publisherEarnedFromCampaign" => {
                let earned = self
                    .global
                    .balances
                    .as_ref()
                    .and_then(|balances| balances.get(&self.global.publisher_id))
                    .cloned()
                    .unwrap_or_default();

                Ok(Value::BigNum(earned))
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
                let alexa_rank = ad_slot.alexa_rank.ok_or(Error::UnknownVariable)?;

                match serde_json::Number::from_f64(alexa_rank) {
                    Some(number) => Ok(Value::Number(number)),
                    None => Err(Error::TypeError),
                }
            }
            _unknown_field => Err(Error::UnknownVariable),
        }
    }
}

fn get_pricing_bounds(channel: &Channel, event_type: &str) -> Pricing {
    channel
        .spec
        .pricing_bounds
        .as_ref()
        .and_then(|pricing_bounds| pricing_bounds.get(event_type))
        .cloned()
        .unwrap_or_else(|| {
            if event_type == "IMPRESSION" {
                Pricing {
                    min: channel.spec.min_per_impression.clone().max(1.into()),
                    max: channel.spec.max_per_impression.clone().max(1.into()),
                }
            } else {
                Pricing {
                    min: 0.into(),
                    max: 0.into(),
                }
            }
        })
}

#[derive(Debug, Clone)]
pub struct AdView {
    pub seconds_since_campaign_impression: u64,
    pub has_custom_preferences: bool,
    pub navigator_language: String,
}

#[derive(Debug, Clone)]
pub struct Global {
    /// Global scope, accessible everywhere
    pub ad_slot_id: String,
    pub ad_slot_type: String,
    pub publisher_id: ValidatorId,
    pub country: Option<String>,
    pub event_type: String,
    pub seconds_since_epoch: u64,
    pub user_agent_os: Option<String>,
    pub user_agent_browser_family: Option<String>,
    /// Global scope, accessible everywhere, campaign-dependant
    pub ad_unit: Option<AdUnit>,
    pub channel: Channel,
    pub status: Option<Status>,
    pub balances: Option<BalancesMap>,
}

#[derive(Debug, Clone)]
pub struct AdSlot {
    pub categories: Vec<String>,
    pub hostname: String,
    pub alexa_rank: Option<f64>,
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

impl Output {
    fn try_get(&self, key: &str) -> Result<Value, Error> {
        match key {
            "show" => Ok(Value::Bool(self.show)),
            "boost" => {
                let boost = Number::from_f64(self.boost).ok_or(Error::TypeError)?;
                Ok(Value::Number(boost))
            },
            price_key if price_key.starts_with("price.") => {
                let price = self.price.get(price_key.trim_start_matches("price.")).ok_or(Error::UnknownVariable)?;
                Ok(Value::BigNum(price.clone()))
            },
            _ => Err(Error::UnknownVariable)
        }
    }
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
    use crate::{
        supermarket::Status,
        util::tests::prep_db::{DUMMY_CHANNEL, IDS},
    };
    use chrono::Utc;

    #[test]
    fn test_try_get_of_input() {
        let ad_unit = AdUnit {
            ipfs: "Hash".to_string(),
            ad_type: "legacy_300x250".to_string(),
            media_url: "media_url".to_string(),
            media_mime: "media_mime".to_string(),
            target_url: "target_url".to_string(),
            targeting: vec![],
            min_targeting_score: None,
            tags: vec![],
            owner: IDS["creator"],
            created: Utc::now(),
            title: None,
            description: None,
            archived: false,
            modified: None,
        };
        let input_balances = BalancesMap::default();
        let mut input = Input {
            ad_view: Some(AdView {
                seconds_since_campaign_impression: 10,
                has_custom_preferences: false,
                navigator_language: "bg".to_string(),
            }),
            global: Global {
                ad_slot_id: "ad_slot_id Value".to_string(),
                ad_slot_type: "ad_slot_type Value".to_string(),
                publisher_id: IDS["leader"],
                country: Some("bg".to_string()),
                event_type: "IMPRESSION".to_string(),
                seconds_since_epoch: 500,
                user_agent_os: Some("os".to_string()),
                user_agent_browser_family: Some("family".to_string()),
                ad_unit: Some(ad_unit),
                channel: DUMMY_CHANNEL.clone(),
                status: Some(Status::Initializing),
                balances: Some(input_balances),
            },
            ad_slot: None,
        };

        let ad_view_seconds_since_show = input
            .try_get("adView.secondsSinceCampaignImpression")
            .expect("Should get the ad_view.seconds_since_campaign_impression field");

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

        assert_eq!(
            Value::BigNum(DUMMY_CHANNEL.deposit_amount.clone()),
            global_campaign_budget
        );

        assert_eq!(
            Err(Error::UnknownVariable),
            input.try_get("adSlot.alexaRank")
        );
        let ad_slot = AdSlot {
            categories: vec![],
            hostname: "".to_string(),
            alexa_rank: Some(20.0),
        };
        input.ad_slot = Some(ad_slot);
        assert!(input.try_get("adSlot.alexaRank").is_ok());
    }

    #[test]
    fn test_try_get_of_output() {
        let output = Output {
            show: false,
            boost: 5.5,
            price: vec![("one".to_string(), 100.into())].into_iter().collect(),
        };

        assert_eq!(Ok(Value::Bool(false)), output.try_get("show"));
        assert_eq!(Ok(Value::Number(Number::from_f64(5.5).expect("Should make a number"))), output.try_get("boost"));
        assert_eq!(Ok(Value::BigNum(100.into())), output.try_get("price.one"));
        assert_eq!(Err(Error::UnknownVariable), output.try_get("price.unknown"));
        assert_eq!(Err(Error::UnknownVariable), output.try_get("unknown"));
    }

    #[test]
    fn test_output_from_channel() {
        use crate::channel::{Pricing, PricingBounds};
        use crate::util::tests::prep_db::DUMMY_CHANNEL;

        let mut channel = DUMMY_CHANNEL.clone();
        channel.spec.pricing_bounds = Some(PricingBounds {
            impression: Some(Pricing {
                min: 1_000.into(),
                max: 2_000.into(),
            }),
            click: Some(Pricing {
                min: 3_000.into(),
                max: 4_000.into(),
            }),
        });

        let output = Output::from(&channel);

        assert_eq!(true, output.show);
        assert_eq!(1.0, output.boost);
        assert_eq!(Some(&BigNum::from(1_000)), output.price.get("IMPRESSION"));
        assert_eq!(Some(&BigNum::from(3_000)), output.price.get("CLICK"));
    }
}
