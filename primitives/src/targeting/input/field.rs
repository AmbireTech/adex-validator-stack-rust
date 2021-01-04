use parse_display::{Display as DeriveDisplay, FromStr as DeriveFromStr};
use serde::{Deserialize, Serialize};
use std::{convert::TryFrom, str::FromStr};

use crate::targeting::Error;

pub const FIELDS: [Field; 24] = [
    // AdView scope, accessible only on the AdView
    Field::AdView(AdView::SecondsSinceCampaignImpression),
    Field::AdView(AdView::HasCustomPreferences),
    Field::AdView(AdView::NavigatorLanguage),
    // Global scope, accessible everywhere
    Field::Global(Global::AdSlotId),
    Field::Global(Global::AdSlotType),
    Field::Global(Global::PublisherId),
    Field::Global(Global::Country),
    Field::Global(Global::EventType),
    Field::Global(Global::SecondsSinceEpoch),
    Field::Global(Global::UserAgentOS),
    Field::Global(Global::UserAgentBrowserFamily),
    // Campaign-dependant - Global scope, accessible everywhere
    // AdUnit
    Field::AdUnit(AdUnit::AdUnitId),
    // Channel
    Field::Channel(Channel::AdvertiserId),
    Field::Channel(Channel::CampaignId),
    Field::Channel(Channel::CampaignSecondsActive),
    Field::Channel(Channel::CampaignSecondsDuration),
    Field::Channel(Channel::CampaignBudget),
    Field::Channel(Channel::EventMinPrice),
    Field::Channel(Channel::EventMaxPrice),
    // Balances
    Field::Balances(Balances::CampaignTotalSpent),
    Field::Balances(Balances::PublisherEarnedFromCampaign),
    // AdSlot scope, accessible on Supermarket and AdView
    Field::AdSlot(AdSlot::Categories),
    Field::AdSlot(AdSlot::Hostname),
    Field::AdSlot(AdSlot::AlexaRank),
];

#[derive(
    Hash, Copy, Clone, Debug, Eq, PartialEq, Serialize, Deserialize, DeriveFromStr, DeriveDisplay,
)]
#[serde(into = "String", try_from = "String")]
pub enum Field {
    /// AdView scope, accessible only on the AdView
    #[display("adView.{0}")]
    AdView(AdView),
    /// Global scope, accessible everywhere
    #[display("{0}")]
    Global(Global),
    /// Global scope, accessible everywhere, campaign-dependant
    #[display("{0}")]
    AdUnit(AdUnit),
    /// Global scope, accessible everywhere, campaign-dependant
    #[display("{0}")]
    Channel(Channel),
    /// Global scope, accessible everywhere, campaign-dependant
    #[display("{0}")]
    Balances(Balances),
    /// AdSlot scope, accessible on Supermarket and AdView
    #[display("adSlot.{0}")]
    AdSlot(AdSlot),
}

impl Into<String> for Field {
    fn into(self) -> String {
        self.to_string()
    }
}

impl TryFrom<String> for Field {
    type Error = Error;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Field::from_str(&value).map_err(|_| Error::UnknownVariable)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Get<G, V> {
    #[serde(skip_deserializing)]
    /// We don't want to deserialize a Getter, we only deserialize Values
    /// This will ensure that we only use a Map of values with `Get::Value` when we deserialize
    /// We also serialize the getter into a Value struct
    Getter(G),
    Value(V),
}

/// We keep the `GetField` implementation on each individual `Get<Getter, Values>` implementation
/// to lower the risk of a field diverging as 2 different `Value` types
pub trait GetField {
    type Output;
    type Field;

    fn get(&self, field: &Self::Field) -> Self::Output;
}

impl<T> GetField for Option<T>
where
    T: GetField,
{
    type Output = Option<T::Output>;
    type Field = T::Field;

    fn get(&self, field: &Self::Field) -> Self::Output {
        self.as_ref().map(|get_field| get_field.get(field))
    }
}

#[derive(Hash, Copy, Clone, Debug, Eq, PartialEq, DeriveFromStr, DeriveDisplay)]
#[display(style = "camelCase")]
pub enum AdUnit {
    AdUnitId,
}

#[derive(Hash, Copy, Clone, Debug, Eq, PartialEq, DeriveFromStr, DeriveDisplay)]
#[display(style = "camelCase")]
pub enum Channel {
    AdvertiserId,
    CampaignId,
    CampaignSecondsActive,
    CampaignSecondsDuration,
    CampaignBudget,
    EventMinPrice,
    EventMaxPrice,
}

impl TryFrom<String> for Channel {
    type Error = parse_display::ParseError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::from_str(&value)
    }
}

impl Into<String> for Channel {
    fn into(self) -> String {
        self.to_string()
    }
}

#[derive(
    Hash, Copy, Clone, Debug, Serialize, Deserialize, Eq, PartialEq, DeriveFromStr, DeriveDisplay,
)]
#[serde(into = "String", try_from = "String")]
#[display(style = "camelCase")]
pub enum Balances {
    CampaignTotalSpent,
    PublisherEarnedFromCampaign,
}

impl TryFrom<String> for Balances {
    type Error = parse_display::ParseError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::from_str(&value)
    }
}

impl Into<String> for Balances {
    fn into(self) -> String {
        self.to_string()
    }
}

#[derive(
    Hash, Copy, Clone, Debug, Serialize, Deserialize, Eq, PartialEq, DeriveFromStr, DeriveDisplay,
)]
#[serde(into = "String", try_from = "String")]
#[display(style = "camelCase")]
pub enum AdSlot {
    Categories,
    Hostname,
    AlexaRank,
}

impl TryFrom<String> for AdSlot {
    type Error = parse_display::ParseError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::from_str(&value)
    }
}

impl Into<String> for AdSlot {
    fn into(self) -> String {
        self.to_string()
    }
}

#[derive(Hash, Copy, Clone, Debug, Eq, PartialEq, DeriveFromStr, DeriveDisplay)]
#[display(style = "camelCase")]
pub enum AdView {
    SecondsSinceCampaignImpression,
    HasCustomPreferences,
    NavigatorLanguage,
}

impl TryFrom<String> for AdView {
    type Error = parse_display::ParseError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::from_str(&value)
    }
}

impl Into<String> for AdView {
    fn into(self) -> String {
        self.to_string()
    }
}

#[derive(
    Hash, Copy, Clone, Debug, Serialize, Deserialize, Eq, PartialEq, DeriveFromStr, DeriveDisplay,
)]
#[display(style = "camelCase")]
#[serde(into = "String", try_from = "String")]
pub enum Global {
    AdSlotId,
    AdSlotType,
    PublisherId,
    Country,
    EventType,
    SecondsSinceEpoch,
    #[display("userAgentOS")]
    UserAgentOS,
    UserAgentBrowserFamily,
}

impl TryFrom<String> for Global {
    type Error = parse_display::ParseError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::from_str(&value)
    }
}

impl Into<String> for Global {
    fn into(self) -> String {
        self.to_string()
    }
}

#[cfg(test)]
mod test {
    use crate::targeting::Value;
    use serde_json::{json, Value as SerdeValue};
    use std::collections::HashMap;

    use super::*;

    fn test_fields() -> HashMap<Field, SerdeValue> {
        vec![
            (
                Field::AdView(AdView::SecondsSinceCampaignImpression),
                json!("adView.secondsSinceCampaignImpression"),
            ),
            (Field::Global(Global::AdSlotId), json!("adSlotId")),
        ]
        .into_iter()
        .collect()
    }

    #[test]
    fn serialize_and_deserialize_field() {
        for (expected_field, value) in test_fields() {
            let actual: Field = serde_json::from_value(value).expect("Should deserialize");

            assert_eq!(expected_field, actual);
        }
    }

    #[test]
    fn from_serde_to_hashmap_of_fields_and_values() {
        let field_1 = Field::AdView(AdView::SecondsSinceCampaignImpression);
        let field_2 = Field::AdView(AdView::HasCustomPreferences);
        let expected: HashMap<Field, Value> = vec![
            (field_1, Value::Number(5.into())),
            (field_2, Value::Bool(true)),
        ]
        .into_iter()
        .collect();

        let map = json!({
            "adView.secondsSinceCampaignImpression": 5,
            "adView.hasCustomPreferences": true
        });

        let actual: HashMap<Field, Value> =
            serde_json::from_value(map).expect("should deserialize");

        assert_eq!(expected, actual);
    }

    fn test_field(field: Field, expected_value: SerdeValue) {
        assert_eq!(
            serde_json::to_value(&field).expect("Should serialize"),
            expected_value
        );
    }

    #[test]
    fn serializes_each_type_of_field_name() {
        test_field(
            Field::AdView(AdView::SecondsSinceCampaignImpression),
            SerdeValue::String("adView.secondsSinceCampaignImpression".into()),
        );
        test_field(
            Field::Global(Global::AdSlotId),
            SerdeValue::String("adSlotId".into()),
        );
        test_field(
            Field::AdUnit(AdUnit::AdUnitId),
            SerdeValue::String("adUnitId".into()),
        );
        test_field(
            Field::Channel(Channel::CampaignBudget),
            SerdeValue::String("campaignBudget".into()),
        );
        test_field(
            Field::Balances(Balances::PublisherEarnedFromCampaign),
            SerdeValue::String("publisherEarnedFromCampaign".into()),
        );
        test_field(
            Field::AdSlot(AdSlot::Hostname),
            SerdeValue::String("adSlot.hostname".into()),
        );
    }
}
