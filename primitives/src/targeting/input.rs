use super::{Error, Value};
use crate::{ToETHChecksum, ValidatorId, IPFS};
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
    pub channel: Option<channel::GetChannel>,
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
    pub fn with_channel(mut self, channel: crate::Channel) -> Self {
        self.channel = Some(Get::Getter(channel::Getter::from_channel(&self, channel)));

        self
    }

    pub fn with_market_channel(
        mut self,
        channel: crate::supermarket::units_for_slot::response::Channel,
    ) -> Self {
        self.channel = Some(Get::Getter(channel::Getter::from_market(channel)));

        self
    }

    pub fn with_balances(mut self, balances: crate::BalancesMap) -> Self {
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

impl Into<Map> for Input {
    fn into(self) -> Map {
        self.to_map()
    }
}

impl GetField for Input {
    type Output = Option<Value>;
    type Field = Field;

    fn get(&self, field: &Self::Field) -> Self::Output {
        match field {
            Field::AdView(ad_view) => self.ad_view.get(ad_view),
            Field::Global(global) => self.global.get(global),
            Field::Channel(channel) => self.channel.get(channel).flatten(),
            Field::Balances(balances) => self.balances.get(balances),
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
/// Global scope, accessible everywhere
pub struct Global {
    pub ad_slot_id: String,
    pub ad_slot_type: String,
    pub publisher_id: ValidatorId,
    pub country: Option<String>,
    pub event_type: String,
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
            field::Global::AdSlotId => Some(Value::String(self.ad_slot_id.clone())),
            field::Global::AdSlotType => Some(Value::String(self.ad_slot_type.clone())),
            field::Global::PublisherId => Some(Value::String(self.publisher_id.to_checksum())),
            field::Global::Country => self.country.clone().map(Value::String),
            field::Global::EventType => Some(Value::String(self.event_type.clone())),
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

pub mod channel {
    use serde::Deserialize;

    use super::{field, Get, GetField, Value};
    use crate::{targeting::get_pricing_bounds, BigNum, ChannelId, ValidatorId};

    pub type GetChannel = Get<Getter, Values>;

    #[derive(Debug, Clone, Deserialize, PartialEq)]
    #[serde(rename_all = "camelCase")]
    pub struct Values {
        pub advertiser_id: ValidatorId,
        pub campaign_id: ChannelId,
        pub campaign_seconds_active: u64,
        pub campaign_seconds_duration: u64,
        pub campaign_budget: BigNum,
        pub event_min_price: Option<BigNum>,
        pub event_max_price: Option<BigNum>,
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct FullChannel {
        pub channel: crate::Channel,
        pub(super) event_type: String,
    }

    #[derive(Debug, Clone, PartialEq)]
    /// The Getter for a Field that requires Channel can be either:
    /// - a Full Channel
    /// - a Channel coming from the Supermarket
    /// Since only the Full Channel can get the pricing bounds,
    /// we wrap the Channel as well as the event_type of the Input
    pub enum Getter {
        Full(FullChannel),
        Market(crate::supermarket::units_for_slot::response::Channel),
    }

    impl Getter {
        /// Input is used to set the Event Type of the Getter
        pub fn from_channel(input: &super::Input, channel: crate::Channel) -> Self {
            Self::Full(FullChannel {
                channel,
                event_type: input.global.event_type.clone(),
            })
        }

        pub fn from_market(channel: crate::supermarket::units_for_slot::response::Channel) -> Self {
            Self::Market(channel)
        }
    }

    impl GetField for Get<Getter, Values> {
        type Output = Option<Value>;
        type Field = field::Channel;

        fn get(&self, field: &Self::Field) -> Self::Output {
            match field {
                field::Channel::AdvertiserId => Some(Value::String(match self {
                    Get::Getter(getter) => match getter {
                        Getter::Full(FullChannel { channel, .. }) => {
                            channel.creator.to_hex_prefix_string()
                        }
                        Getter::Market(s_channel) => s_channel.creator.to_hex_prefix_string(),
                    },
                    Get::Value(Values { advertiser_id, .. }) => {
                        advertiser_id.to_hex_prefix_string()
                    }
                })),
                field::Channel::CampaignId => Some(Value::String(match self {
                    Get::Getter(getter) => match getter {
                        Getter::Full(FullChannel { channel, .. }) => channel.id.to_string(),
                        Getter::Market(s_channel) => s_channel.id.to_string(),
                    },
                    Get::Value(Values { campaign_id, .. }) => campaign_id.to_string(),
                })),
                field::Channel::CampaignSecondsActive => Some(Value::Number(match self {
                    Get::Getter(getter) => {
                        let (active_from, created) = match getter {
                            Getter::Full(FullChannel { channel, .. }) => {
                                (channel.spec.active_from, channel.spec.created)
                            }
                            Getter::Market(s_channel) => {
                                (s_channel.spec.active_from, s_channel.spec.created)
                            }
                        };

                        let duration = chrono::Utc::now() - active_from.unwrap_or(created);

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
                field::Channel::CampaignSecondsDuration => Some(Value::Number(match self {
                    Get::Getter(getter) => {
                        let (withdraw_period_start, active_from, created) = match getter {
                            Getter::Full(FullChannel { channel, .. }) => (
                                channel.spec.withdraw_period_start,
                                channel.spec.active_from,
                                channel.spec.created,
                            ),
                            Getter::Market(s_channel) => (
                                s_channel.spec.withdraw_period_start,
                                s_channel.spec.active_from,
                                s_channel.spec.created,
                            ),
                        };

                        let duration = withdraw_period_start - active_from.unwrap_or(created);

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
                field::Channel::CampaignBudget => Some(Value::BigNum(match self {
                    Get::Getter(getter) => match getter {
                        Getter::Full(FullChannel { channel, .. }) => channel.deposit_amount.clone(),
                        Getter::Market(s_channel) => s_channel.deposit_amount.clone(),
                    },
                    Get::Value(Values {
                        campaign_budget, ..
                    }) => campaign_budget.clone(),
                })),
                field::Channel::EventMinPrice => match self {
                    Get::Getter(Getter::Full(FullChannel {
                        channel,
                        event_type,
                    })) => Some(Value::BigNum(get_pricing_bounds(channel, event_type).min)),
                    // The supermarket Channel, does not have enough information to return the event_min_price
                    Get::Getter(Getter::Market(_)) => None,
                    Get::Value(Values {
                        event_min_price, ..
                    }) => event_min_price.clone().map(Value::BigNum),
                },
                field::Channel::EventMaxPrice => match self {
                    Get::Getter(Getter::Full(FullChannel {
                        channel,
                        event_type,
                    })) => Some(Value::BigNum(get_pricing_bounds(channel, event_type).max)),
                    // The supermarket Channel, does not have enough information to return the event_max_price
                    Get::Getter(Getter::Market(_)) => None,
                    Get::Value(Values {
                        event_max_price, ..
                    }) => event_max_price.clone().map(Value::BigNum),
                },
            }
        }
    }
}

pub mod balances {
    use super::{field, Get, GetField, Value};
    use crate::{BalancesMap, BigNum, ValidatorId};
    use serde::Deserialize;

    pub type GetBalances = Get<Getter, Values>;

    #[derive(Debug, Clone, Deserialize, PartialEq)]
    #[serde(rename_all = "camelCase")]
    pub struct Values {
        pub campaign_total_spent: BigNum,
        pub publisher_earned_from_campaign: BigNum,
    }

    #[derive(Debug, Clone, PartialEq)]
    pub struct Getter {
        pub balances: BalancesMap,
        pub(super) publisher_id: ValidatorId,
    }

    impl GetField for Get<Getter, Values> {
        type Output = Value;
        type Field = field::Balances;

        fn get(&self, field: &Self::Field) -> Value {
            match field {
                field::Balances::CampaignTotalSpent => Value::BigNum(match self {
                    Get::Getter(Getter { balances, .. }) => balances.values().sum(),
                    Get::Value(Values {
                        campaign_total_spent,
                        ..
                    }) => campaign_total_spent.clone(),
                }),
                field::Balances::PublisherEarnedFromCampaign => Value::BigNum(match self {
                    Get::Getter(Getter {
                        balances,
                        publisher_id,
                    }) => balances.get(publisher_id).cloned().unwrap_or_default(),
                    Get::Value(Values {
                        publisher_earned_from_campaign,
                        ..
                    }) => publisher_earned_from_campaign.clone(),
                }),
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    pub use crate::{
        util::tests::prep_db::{DUMMY_CHANNEL as CHANNEL, DUMMY_IPFS as IPFS, IDS},
        AdUnit, BalancesMap,
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
            "publisherId": "0xB7d3F81E857692d13e9D63b232A90F4A1793189E",
            "country": "BG",
            "eventType": "IMPRESSION",
            // 06/06/2020 @ 12:00pm (UTC)
            "secondsSinceEpoch": 1591444800,
            "userAgentOS": "Ubuntu",
            "userAgentBrowserFamily": "Firefox",
            // Global scope, accessible everywhere, campaign-dependant
            "adUnitId": "Qmasg8FrbuSQpjFu3kRnZF9beg8rEBFrqgi1uXDRwCbX5f",
            "advertiserId": "0x033ed90e0fec3f3ea1c9b005c724d704501e0196",
            "campaignId": "0x061d5e2a67d0a9a10f1c732bca12a676d83f79663a396f7d87b3e30b9b411088",
            "campaignTotalSpent": "40",
            "campaignSecondsActive": 40633521,
            "campaignSecondsDuration": 2509030800_u64,
            "campaignBudget": "1000",
            "eventMinPrice": "1",
            "eventMaxPrice": "10",
            "publisherEarnedFromCampaign": "30",
            // adSlot scope, accessible on Supermarket and AdView
            "adSlot.categories": ["IAB3", "IAB13-7", "IAB5"],
            "adSlot.hostname": "adex.network",
            "adSlot.alexaRank": 2.0,
        });

        let actual_date = Utc.ymd(2020, 6, 6).and_hms(12, 0, 0);

        let balances: BalancesMap = vec![(IDS["publisher"], 30.into()), (IDS["leader"], 10.into())]
            .into_iter()
            .collect();

        let full_input = Input {
            ad_view: Some(AdView {
                seconds_since_campaign_impression: 10,
                has_custom_preferences: true,
                navigator_language: "en".into(),
            }),
            global: Global {
                ad_slot_id: IPFS[0].to_string(),
                ad_slot_type: "legacy_300x100".into(),
                publisher_id: IDS["publisher"],
                country: Some("BG".into()),
                event_type: "IMPRESSION".into(),
                seconds_since_epoch: actual_date,
                user_agent_os: Some("Ubuntu".into()),
                user_agent_browser_family: Some("Firefox".into()),
            },
            // Channel can only be tested with a Value, since the campaign_seconds_* are calculated based on current DateTime
            channel: Some(Get::Value(channel::Values {
                advertiser_id: CHANNEL.creator,
                campaign_id: CHANNEL.id,
                campaign_seconds_active: 40633521,
                campaign_seconds_duration: 2509030800,
                campaign_budget: CHANNEL.deposit_amount.clone(),
                event_min_price: Some(CHANNEL.spec.min_per_impression.clone()),
                event_max_price: Some(CHANNEL.spec.max_per_impression.clone()),
            })),
            balances: Some(Get::Getter(balances::Getter {
                balances,
                publisher_id: IDS["publisher"],
            })),
            ad_unit_id: Some(IPFS[1].clone()),
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
