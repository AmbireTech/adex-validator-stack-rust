use crate::{
    targeting::Rules, AdUnit, Address, Channel, EventSubmission, UnifiedNum, Validator,
    ValidatorDesc, ValidatorId,
};

use chrono::{
    serde::{ts_milliseconds, ts_milliseconds_option},
    DateTime, Utc,
};
use serde::{Deserialize, Serialize};
use serde_with::with_prefix;

pub use {
    campaign_id::CampaignId,
    pricing::{Pricing, PricingBounds},
    validators::Validators,
};

with_prefix!(pub prefix_active "active_");

mod campaign_id {
    use crate::ToHex;
    use hex::{FromHex, FromHexError};
    use serde::{
        de::{self, Visitor},
        Deserialize, Deserializer, Serialize, Serializer,
    };
    use std::{fmt, str::FromStr};
    use thiserror::Error;
    use uuid::Uuid;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    /// an Id of 16 bytes, (de)serialized as a `0x` prefixed hex
    /// In this implementation of the `CampaignId` the value is generated from a `Uuid::new_v4().to_simple()`
    pub struct CampaignId([u8; 16]);

    impl CampaignId {
        /// Generates randomly a `CampaignId` using `Uuid::new_v4().to_simple()`
        pub fn new() -> Self {
            Self::default()
        }

        pub fn as_bytes(&self) -> &[u8; 16] {
            &self.0
        }

        pub fn from_bytes(bytes: &[u8; 16]) -> Self {
            Self(*bytes)
        }
    }

    impl Default for CampaignId {
        fn default() -> Self {
            Self(*Uuid::new_v4().as_bytes())
        }
    }

    impl AsRef<[u8]> for CampaignId {
        fn as_ref(&self) -> &[u8] {
            &self.0
        }
    }

    impl AsRef<[u8; 16]> for CampaignId {
        fn as_ref(&self) -> &[u8; 16] {
            &self.0
        }
    }

    #[derive(Debug, Error)]
    pub enum Error {
        /// the `0x` prefix is missing
        #[error("Expected a `0x` prefix")]
        ExpectedPrefix,
        #[error(transparent)]
        InvalidHex(#[from] FromHexError),
    }

    impl FromStr for CampaignId {
        type Err = Error;

        fn from_str(s: &str) -> Result<Self, Self::Err> {
            match s.strip_prefix("0x") {
                Some(hex) => Ok(Self(<[u8; 16]>::from_hex(hex)?)),
                None => Err(Error::ExpectedPrefix),
            }
        }
    }

    impl fmt::Display for CampaignId {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str(&self.0.to_hex_prefixed())
        }
    }

    impl Serialize for CampaignId {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            serializer.serialize_str(&self.0.to_hex_prefixed())
        }
    }

    impl<'de> Deserialize<'de> for CampaignId {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            deserializer.deserialize_str(StringIdVisitor)
        }
    }

    struct StringIdVisitor;

    impl<'de> Visitor<'de> for StringIdVisitor {
        type Value = CampaignId;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a string of a `0x` prefixed hex with 16 bytes")
        }

        fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            value
                .parse::<CampaignId>()
                .map_err(|err| E::custom(err.to_string()))
        }

        fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
        where
            E: de::Error,
        {
            self.visit_str(&value)
        }
    }

    #[cfg(test)]
    mod test {
        use serde_json::{to_value, Value};

        use super::*;

        #[test]
        fn de_serializes_campaign_id() {
            let id = CampaignId::new();

            assert_eq!(
                Value::String(id.0.to_hex_prefixed()),
                to_value(id).expect("Should serialize")
            );
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Campaign {
    pub id: CampaignId,
    pub channel: Channel,
    pub creator: Address,
    pub budget: UnifiedNum,
    pub validators: Validators,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Event pricing bounds
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pricing_bounds: Option<PricingBounds>,
    /// EventSubmission object, applies to event submission (POST /channel/:id/events)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_submission: Option<EventSubmission>,
    /// An array of AdUnit (optional)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ad_units: Vec<AdUnit>,
    #[serde(default)]
    pub targeting_rules: Rules,
    /// A millisecond timestamp of when the campaign was created
    #[serde(with = "ts_milliseconds")]
    pub created: DateTime<Utc>,
    /// A millisecond timestamp representing the time you want this campaign to become active (optional)
    /// Used by the AdViewManager & Targeting AIP#31
    #[serde(flatten, with = "prefix_active")]
    pub active: Active,
}

impl Campaign {
    pub fn find_validator(&self, validator: &ValidatorId) -> Option<Validator<&ValidatorDesc>> {
        match (self.leader(), self.follower()) {
            (Some(leader), _) if &leader.id == validator => Some(Validator::Leader(leader)),
            (_, Some(follower)) if &follower.id == validator => Some(Validator::Follower(follower)),
            _ => None,
        }
    }

    /// Matches the Channel.leader to the Campaign.validators.leader
    /// If they match it returns `Some`, otherwise, it returns `None`
    pub fn leader(&self) -> Option<&'_ ValidatorDesc> {
        self.validators.find(&self.channel.leader)
    }

    /// Matches the Channel.follower to the Campaign.spec.follower
    /// If they match it returns `Some`, otherwise, it returns `None`
    pub fn follower(&self) -> Option<&'_ ValidatorDesc> {
        self.validators.find(&self.channel.follower)
    }

    /// Returns the pricing of a given event
    pub fn pricing(&self, event: &str) -> Option<&Pricing> {
        self.pricing_bounds
            .as_ref()
            .and_then(|bound| bound.get(event))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Active {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "ts_milliseconds_option"
    )]
    pub from: Option<DateTime<Utc>>,
    //
    // TODO: AIP#61 Update docs
    //
    /// A millisecond timestamp of when the campaign should enter a withdraw period
    /// (no longer accept any events other than CHANNEL_CLOSE)
    /// A sane value should be lower than channel.validUntil * 1000 and higher than created
    /// It's recommended to set this at least one month prior to channel.validUntil * 1000
    #[serde(with = "ts_milliseconds")]
    pub to: DateTime<Utc>,
}

mod pricing {
    use crate::UnifiedNum;
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
    pub struct Pricing {
        pub min: UnifiedNum,
        pub max: UnifiedNum,
    }

    #[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
    #[serde(rename_all = "UPPERCASE")]
    pub struct PricingBounds {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub impression: Option<Pricing>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub click: Option<Pricing>,
    }

    impl PricingBounds {
        pub fn to_vec(&self) -> Vec<(&str, Pricing)> {
            let mut vec = Vec::new();

            if let Some(pricing) = self.impression.as_ref() {
                vec.push(("IMPRESSION", pricing.clone()));
            }

            if let Some(pricing) = self.click.as_ref() {
                vec.push(("CLICK", pricing.clone()))
            }

            vec
        }

        pub fn get(&self, event_type: &str) -> Option<&Pricing> {
            match event_type {
                "IMPRESSION" => self.impression.as_ref(),
                "CLICK" => self.click.as_ref(),
                _ => None,
            }
        }
    }
}
/// Campaign Validators
pub mod validators {
    use std::ops::Index;

    use crate::{ValidatorDesc, ValidatorId};
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
    /// Unordered list of the validators representing the leader & follower
    pub struct Validators(ValidatorDesc, ValidatorDesc);

    impl Validators {
        pub fn new(validators: (ValidatorDesc, ValidatorDesc)) -> Self {
            Self(validators.0, validators.1)
        }

        pub fn find(&self, validator_id: &ValidatorId) -> Option<&ValidatorDesc> {
            if &self.0.id == validator_id {
                Some(&self.0)
            } else if &self.1.id == validator_id {
                Some(&self.1)
            } else {
                None
            }
        }

        pub fn iter(&self) -> Iter<'_> {
            Iter::new(self)
        }
    }

    impl From<(ValidatorDesc, ValidatorDesc)> for Validators {
        fn from(validators: (ValidatorDesc, ValidatorDesc)) -> Self {
            Self(validators.0, validators.1)
        }
    }

    impl Index<usize> for Validators {
        type Output = ValidatorDesc;
        fn index(&self, index: usize) -> &Self::Output {
            match index {
                0 => &self.0,
                1 => &self.1,
                _ => panic!("Validators index is out of bound")
            }
        }
    }

    /// Fixed size iterator of 2, as we need an iterator in couple of occasions
    impl<'a> IntoIterator for &'a Validators {
        type Item = &'a ValidatorDesc;
        type IntoIter = Iter<'a>;

        fn into_iter(self) -> Self::IntoIter {
            self.iter()
        }
    }

    pub struct Iter<'a> {
        validators: &'a Validators,
        index: u8,
    }

    impl<'a> Iter<'a> {
        fn new(validators: &'a Validators) -> Self {
            Self {
                validators,
                index: 0,
            }
        }
    }

    impl<'a> Iterator for Iter<'a> {
        type Item = &'a ValidatorDesc;

        fn next(&mut self) -> Option<Self::Item> {
            match self.index {
                0 => {
                    self.index += 1;

                    Some(&self.validators.0)
                }
                1 => {
                    self.index += 1;

                    Some(&self.validators.1)
                }
                _ => None,
            }
        }
    }
}

#[cfg(feature = "postgres")]
mod postgres {
    use crate::Channel;

    use super::{Active, Campaign, CampaignId, PricingBounds, Validators};
    use bytes::BytesMut;
    use postgres_types::{accepts, to_sql_checked, FromSql, IsNull, Json, ToSql, Type};
    use std::error::Error;
    use tokio_postgres::Row;

    impl From<&Row> for Campaign {
        fn from(row: &Row) -> Self {
            Self {
                id: row.get("id"),
                channel: Channel::from(row),
                creator: row.get("creator"),
                budget: row.get("budget"),
                validators: row.get("validators"),
                title: row.get("title"),
                pricing_bounds: row.get("pricing_bounds"),
                event_submission: row.get("event_submission"),
                ad_units: row.get::<_, Json<_>>("ad_units").0,
                targeting_rules: row.get("targeting_rules"),
                created: row.get("created"),
                active: Active {
                    from: row.get("active_from"),
                    to: row.get("active_to"),
                },
            }
        }
    }

    impl<'a> FromSql<'a> for CampaignId {
        fn from_sql(ty: &Type, raw: &'a [u8]) -> Result<Self, Box<dyn Error + Sync + Send>> {
            let str_slice = <&str as FromSql>::from_sql(ty, raw)?;

            Ok(str_slice.parse()?)
        }

        accepts!(TEXT, VARCHAR);
    }

    impl From<&Row> for CampaignId {
        fn from(row: &Row) -> Self {
            row.get("id")
        }
    }

    impl ToSql for CampaignId {
        fn to_sql(
            &self,
            ty: &Type,
            w: &mut BytesMut,
        ) -> Result<IsNull, Box<dyn Error + Sync + Send>> {
            self.to_string().to_sql(ty, w)
        }

        accepts!(TEXT, VARCHAR);
        to_sql_checked!();
    }

    impl<'a> FromSql<'a> for Validators {
        fn from_sql(ty: &Type, raw: &'a [u8]) -> Result<Self, Box<dyn Error + Sync + Send>> {
            let json = <Json<Self> as FromSql>::from_sql(ty, raw)?;

            Ok(json.0)
        }

        accepts!(JSONB);
    }

    impl ToSql for Validators {
        fn to_sql(
            &self,
            ty: &Type,
            w: &mut BytesMut,
        ) -> Result<IsNull, Box<dyn Error + Sync + Send>> {
            Json(self).to_sql(ty, w)
        }

        accepts!(JSONB);
        to_sql_checked!();
    }

    impl<'a> FromSql<'a> for PricingBounds {
        fn from_sql(ty: &Type, raw: &'a [u8]) -> Result<Self, Box<dyn Error + Sync + Send>> {
            let json = <Json<Self> as FromSql>::from_sql(ty, raw)?;

            Ok(json.0)
        }

        accepts!(JSONB);
    }

    impl ToSql for PricingBounds {
        fn to_sql(
            &self,
            ty: &Type,
            w: &mut BytesMut,
        ) -> Result<IsNull, Box<dyn Error + Sync + Send>> {
            Json(self).to_sql(ty, w)
        }

        accepts!(JSONB);
        to_sql_checked!();
    }
}
