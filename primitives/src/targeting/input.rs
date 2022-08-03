use self::campaign::FullCampaign;

use super::{Error, Value};
use crate::{sentry::EventType, Address, IPFS};
use chrono::{serde::ts_seconds, DateTime, Utc};
use serde::{Deserialize, Serialize};

use field::{Field, GetField};

serde_with::with_prefix!(adview_prefix "adView.");
serde_with::with_prefix!(adslot_prefix "adSlot.");

pub type Map = serde_json::Map<String, serde_json::Value>;

pub mod field;

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum Get<G, V> {
    #[serde(skip_deserializing)]
    /// We don't want to deserialize a Getter, we only deserialize Values
    /// This will ensure that we only use a Map of values with `Get::Value` when we deserialize
    Getter(G),
    Value(V),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
/// Input is deserializable from the struct, however we should be careful,
/// since all the fields should align with the enum Field
#[serde(rename_all = "camelCase", into = "Map")]
pub struct Input {
    /// AdView scope, accessible only on the AdView
    #[serde(flatten, with = "adview_prefix")]
    pub ad_view: Option<AdView>,
    /// Global scope, accessible everywhere
    #[serde(flatten)]
    pub global: Global,
    #[serde(flatten)]
    pub campaign: Option<campaign::GetCampaign>,
    #[serde(flatten)]
    pub balances: Option<balances::GetBalances>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ad_unit_id: Option<IPFS>,
    /// adSlot scope, accessible on Supermarket and AdView
    #[serde(flatten, with = "adslot_prefix")]
    pub ad_slot: Option<AdSlot>,
}

impl Input {
    /// Sets the Channel Getter
    pub fn with_campaign(mut self, campaign: crate::Campaign) -> Self {
        self.campaign = Some(Get::Getter(FullCampaign {
            campaign,
            event_type: self.global.event_type,
        }));

        self
    }

    pub fn with_balances(mut self, balances: crate::UnifiedMap) -> Self {
        self.balances = Some(Get::Getter(balances::Getter {
            balances,
            publisher_id: self.global.publisher_id,
        }));

        self
    }

    /// This method will try to parse the `Field` from the string
    /// then it will get the field value, but there isn't one,
    /// it will return `Error::UnknownVariable`, otherwise it will return the value
    pub fn try_get(&self, field: &str) -> Result<Value, Error> {
        let field = field.parse::<Field>().map_err(|_| Error::UnknownVariable)?;

        self.get(&field).ok_or(Error::UnknownVariable)
    }

    pub fn to_map(&self) -> Map {
        field::FIELDS
            .iter()
            .filter_map(|field| {
                self.get(field)
                    .map(|value| (field.to_string(), value.into()))
            })
            .collect()
    }
}

impl From<Input> for Map {
    fn from(input: Input) -> Self {
        input.to_map()
    }
}

impl GetField for Input {
    type Output = Option<Value>;
    type Field = Field;

    fn get(&self, field: &Self::Field) -> Self::Output {
        match field {
            Field::AdView(ad_view) => self.ad_view.get(ad_view),
            Field::Global(global) => self.global.get(global),
            Field::Campaign(channel) => self.campaign.get(channel).flatten(),
            Field::Balances(balances) => self.balances.get(balances).flatten(),
            Field::AdSlot(ad_slot) => self.ad_slot.get(ad_slot).flatten(),
            Field::AdUnit(ad_unit) => match ad_unit {
                field::AdUnit::AdUnitId => self
                    .ad_unit_id
                    .as_ref()
                    .map(|ipfs| Value::String(ipfs.to_string())),
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AdView {
    pub seconds_since_campaign_impression: u64,
    pub has_custom_preferences: bool,
    pub navigator_language: String,
}

impl GetField for AdView {
    type Output = Value;
    type Field = field::AdView;

    fn get(&self, field: &Self::Field) -> Self::Output {
        match field {
            field::AdView::SecondsSinceCampaignImpression => {
                Value::Number(self.seconds_since_campaign_impression.into())
            }
            field::AdView::HasCustomPreferences => Value::Bool(self.has_custom_preferences),
            field::AdView::NavigatorLanguage => Value::String(self.navigator_language.clone()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
/// Global scope, accessible everywhere
pub struct Global {
    /// We still use `String`, because the `Event`s have an `Option`al `AdSlot` value.
    pub ad_slot_id: IPFS,
    pub ad_slot_type: String,
    pub publisher_id: Address,
    pub country: Option<String>,
    pub event_type: EventType,
    #[serde(with = "ts_seconds")]
    pub seconds_since_epoch: DateTime<Utc>,
    #[serde(rename = "userAgentOS")]
    pub user_agent_os: Option<String>,
    pub user_agent_browser_family: Option<String>,
}

impl GetField for Global {
    type Output = Option<Value>;
    type Field = field::Global;

    fn get(&self, field: &Self::Field) -> Self::Output {
        match field {
            field::Global::AdSlotId => Some(Value::String(self.ad_slot_id.to_string())),
            field::Global::AdSlotType => Some(Value::String(self.ad_slot_type.clone())),
            field::Global::PublisherId => Some(Value::String(self.publisher_id.to_string())),
            field::Global::Country => self.country.clone().map(Value::String),
            field::Global::EventType => Some(Value::String(self.event_type.to_string())),
            field::Global::SecondsSinceEpoch => {
                // no need to convert to u64, this value should always be positive
                Some(Value::new_number(self.seconds_since_epoch.timestamp()))
            }
            field::Global::UserAgentOS => self.user_agent_os.clone().map(Value::String),
            field::Global::UserAgentBrowserFamily => {
                self.user_agent_browser_family.clone().map(Value::String)
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
/// AdSlot scope, accessible on Supermarket and AdView
pub struct AdSlot {
    pub categories: Vec<String>,
    pub hostname: String,
    pub alexa_rank: Option<f64>,
}

impl GetField for AdSlot {
    type Output = Option<Value>;
    type Field = field::AdSlot;

    fn get(&self, field: &Self::Field) -> Self::Output {
        match field {
            field::AdSlot::Categories => Some(Value::Array(
                self.categories
                    .iter()
                    .map(|string| Value::String(string.clone()))
                    .collect(),
            )),
            field::AdSlot::Hostname => Some(Value::String(self.hostname.clone())),
            field::AdSlot::AlexaRank => self
                .alexa_rank
                .and_then(serde_json::Number::from_f64)
                .map(Value::Number),
        }
    }
}

pub mod campaign {
    use serde::Deserialize;

    use super::{field, Get, GetField, Value};
    use crate::{
        sentry::EventType, targeting::get_pricing_bounds, Address, CampaignId, ToHex, UnifiedNum,
    };

    pub type GetCampaign = Get<FullCampaign, Values>;

    #[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
    #[serde(rename_all = "camelCase")]
    pub struct Values {
        pub advertiser_id: Address,
        pub campaign_id: CampaignId,
        pub campaign_seconds_active: u64,
        pub campaign_seconds_duration: u64,
        pub campaign_budget: UnifiedNum,
        pub event_min_price: Option<UnifiedNum>,
        pub event_max_price: Option<UnifiedNum>,
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct FullCampaign {
        pub campaign: crate::Campaign,
        pub(super) event_type: EventType,
    }

    impl GetField for Get<FullCampaign, Values> {
        type Output = Option<Value>;
        type Field = field::Campaign;

        fn get(&self, field: &Self::Field) -> Self::Output {
            match field {
                field::Campaign::AdvertiserId => Some(Value::String(match self {
                    Get::Getter(FullCampaign { campaign, .. }) => campaign.creator.to_string(),
                    Get::Value(Values { advertiser_id, .. }) => advertiser_id.to_string(),
                })),
                field::Campaign::CampaignId => Some(Value::String(match self {
                    Get::Getter(FullCampaign { campaign, .. }) => campaign.id.to_hex_prefixed(),
                    Get::Value(Values { campaign_id, .. }) => campaign_id.to_hex_prefixed(),
                })),
                field::Campaign::CampaignSecondsActive => Some(Value::Number(match self {
                    Get::Getter(FullCampaign { campaign, .. }) => {
                        let duration =
                            chrono::Utc::now() - campaign.active.from.unwrap_or(campaign.created);

                        let seconds = duration
                            .to_std()
                            .map(|duration| duration.as_secs())
                            .unwrap_or(0);

                        seconds.into()
                    }
                    Get::Value(Values {
                        campaign_seconds_active,
                        ..
                    }) => (*campaign_seconds_active).into(),
                })),
                field::Campaign::CampaignSecondsDuration => Some(Value::Number(match self {
                    Get::Getter(FullCampaign { campaign, .. }) => {
                        let duration =
                            campaign.active.to - campaign.active.from.unwrap_or(campaign.created);

                        let seconds = duration
                            .to_std()
                            .map(|std_duration| std_duration.as_secs())
                            .unwrap_or(0);

                        seconds.into()
                    }
                    Get::Value(Values {
                        campaign_seconds_duration,
                        ..
                    }) => (*campaign_seconds_duration).into(),
                })),
                field::Campaign::CampaignBudget => Some(Value::UnifiedNum(match self {
                    Get::Getter(FullCampaign { campaign, .. }) => campaign.budget,
                    Get::Value(Values {
                        campaign_budget, ..
                    }) => *campaign_budget,
                })),
                field::Campaign::EventMinPrice => match self {
                    Get::Getter(FullCampaign {
                        campaign,
                        event_type,
                    }) => Some(Value::UnifiedNum(
                        get_pricing_bounds(campaign, event_type).min,
                    )),
                    Get::Value(Values {
                        event_min_price, ..
                    }) => event_min_price.map(Value::UnifiedNum),
                },
                field::Campaign::EventMaxPrice => match self {
                    Get::Getter(FullCampaign {
                        campaign,
                        event_type,
                    }) => Some(Value::UnifiedNum(
                        get_pricing_bounds(campaign, event_type).max,
                    )),
                    Get::Value(Values {
                        event_max_price, ..
                    }) => event_max_price.map(Value::UnifiedNum),
                },
            }
        }
    }
}

pub mod balances {
    use super::{field, Get, GetField, Value};
    use crate::{Address, UnifiedMap, UnifiedNum};
    use serde::Deserialize;

    pub type GetBalances = Get<Getter, Values>;

    #[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
    #[serde(rename_all = "camelCase")]
    pub struct Values {
        pub campaign_total_spent: UnifiedNum,
        pub publisher_earned_from_campaign: UnifiedNum,
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Getter {
        pub balances: UnifiedMap,
        pub(super) publisher_id: Address,
    }

    impl GetField for Get<Getter, Values> {
        type Output = Option<Value>;
        type Field = field::Balances;

        fn get(&self, field: &Self::Field) -> Self::Output {
            match field {
                field::Balances::CampaignTotalSpent => match self {
                    Get::Getter(Getter { balances, .. }) => balances
                        .values()
                        .sum::<Option<UnifiedNum>>()
                        .map(Value::UnifiedNum),
                    Get::Value(Values {
                        campaign_total_spent,
                        ..
                    }) => Some(Value::UnifiedNum(*campaign_total_spent)),
                },
                // Leave the default of `0` if the publisher is not found in the balances.
                field::Balances::PublisherEarnedFromCampaign => {
                    Some(Value::UnifiedNum(match self {
                        Get::Getter(Getter {
                            balances,
                            publisher_id,
                        }) => balances.get(publisher_id).cloned().unwrap_or_default(),
                        Get::Value(Values {
                            publisher_earned_from_campaign,
                            ..
                        }) => *publisher_earned_from_campaign,
                    }))
                }
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        sentry::IMPRESSION,
        test_util::{LEADER, PUBLISHER},
    };
    pub use crate::{
        test_util::{DUMMY_CAMPAIGN as CAMPAIGN, DUMMY_IPFS as IPFS},
        AdUnit, UnifiedMap,
    };
    use chrono::{TimeZone, Utc};
    use serde_json::json;

    #[test]
    fn input_serialization_and_deserialization() {
        let full_json = json!({
            // Global scope, accessible everywhere
            "adView.secondsSinceCampaignImpression": 10,
            "adView.hasCustomPreferences": true,
            "adView.navigatorLanguage": "en",
            "adSlotId": "QmcUVX7fvoLMM93uN2bD3wGTH8MXSxeL8hojYfL2Lhp7mR",
            "adSlotType": "legacy_300x100",
            "publisherId": "0xE882ebF439207a70dDcCb39E13CA8506c9F45fD9",
            "country": "BG",
            "eventType": "IMPRESSION",
            // 06/06/2020 @ 12:00pm (UTC)
            "secondsSinceEpoch": 1591444800,
            "userAgentOS": "Ubuntu",
            "userAgentBrowserFamily": "Firefox",
            // Global scope, accessible everywhere, campaign-dependant
            "adUnitId": "Qmasg8FrbuSQpjFu3kRnZF9beg8rEBFrqgi1uXDRwCbX5f",
            "advertiserId": "0xaCBaDA2d5830d1875ae3D2de207A1363B316Df2F",
            "campaignId": "0x936da01f9abd4d9d80c702af85c822a8",
            "campaignTotalSpent": "40",
            "campaignSecondsActive": 40633521,
            "campaignSecondsDuration": 2509030800_u64,
            "campaignBudget": "100000000000",
            "eventMinPrice": "1",
            "eventMaxPrice": "10",
            "publisherEarnedFromCampaign": "30",
            // adSlot scope, accessible on Supermarket and AdView
            "adSlot.categories": ["IAB3", "IAB13-7", "IAB5"],
            "adSlot.hostname": "adex.network",
            "adSlot.alexaRank": 2.0,
        });

        let actual_date = Utc.ymd(2020, 6, 6).and_hms(12, 0, 0);

        let balances: UnifiedMap = vec![(*PUBLISHER, 30.into()), (*LEADER, 10.into())]
            .into_iter()
            .collect();

        let full_input = Input {
            ad_view: Some(AdView {
                seconds_since_campaign_impression: 10,
                has_custom_preferences: true,
                navigator_language: "en".into(),
            }),
            global: Global {
                ad_slot_id: IPFS[0],
                ad_slot_type: "legacy_300x100".into(),
                publisher_id: *PUBLISHER,
                country: Some("BG".into()),
                event_type: IMPRESSION,
                seconds_since_epoch: actual_date,
                user_agent_os: Some("Ubuntu".into()),
                user_agent_browser_family: Some("Firefox".into()),
            },
            // Channel can only be tested with a Value, since the campaign_seconds_* are calculated based on current DateTime
            campaign: Some(Get::Value(campaign::Values {
                advertiser_id: CAMPAIGN.creator,
                campaign_id: CAMPAIGN.id,
                campaign_seconds_active: 40633521,
                campaign_seconds_duration: 2509030800,
                campaign_budget: CAMPAIGN.budget,
                event_min_price: Some(
                    CAMPAIGN
                        .pricing(IMPRESSION)
                        .map(|price| price.min)
                        .expect("should have price"),
                ),
                event_max_price: Some(
                    CAMPAIGN
                        .pricing(IMPRESSION)
                        .map(|price| price.max)
                        .expect("Should have price"),
                ),
            })),
            balances: Some(Get::Getter(balances::Getter {
                balances,
                publisher_id: *PUBLISHER,
            })),
            ad_unit_id: Some(IPFS[1]),
            ad_slot: Some(AdSlot {
                categories: vec!["IAB3".into(), "IAB13-7".into(), "IAB5".into()],
                hostname: "adex.network".into(),
                alexa_rank: Some(2.0),
            }),
        };

        let ser_actual_json = serde_json::to_value(full_input.clone()).expect("Should serialize");

        pretty_assertions::assert_eq!(full_json, ser_actual_json);
        pretty_assertions::assert_eq!(full_json.to_string(), ser_actual_json.to_string());

        let de_actual_input =
            serde_json::from_value::<Input>(ser_actual_json).expect("Should deserialize");

        // Compare Map, instead of Input, since Getters are serialized as Values and we cannot compare Inputs.
        let expected_map: Map = full_input.into();
        let actual_map: Map = de_actual_input.into();

        pretty_assertions::assert_eq!(
            expected_map,
            actual_map,
            "Comparing the output Maps of the Inputs failed"
        );
    }
}
