use crate::{
    channel_v5::Channel, targeting::Rules, AdUnit, Address, EventSubmission, UnifiedNum,
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
    validators::{ValidatorRole, Validators},
};

with_prefix!(prefix_active "active_");

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
    pub fn find_validator(&self, validator: ValidatorId) -> Option<&'_ ValidatorDesc> {
        match (self.leader(), self.follower()) {
            (Some(leader), _) if leader.id == validator => Some(leader),
            (_, Some(follower)) if follower.id == validator => Some(follower),
            _ => None,
        }
    }

    /// Matches the Channel.leader to the Campaign.spec.leader
    /// If they match it returns `Some`, otherwise, it returns `None`
    pub fn leader(&self) -> Option<&'_ ValidatorDesc> {
        if self.channel.leader == self.validators.leader().id {
            Some(self.validators.leader())
        } else {
            None
        }
    }

    /// Matches the Channel.follower to the Campaign.spec.follower
    /// If they match it returns `Some`, otherwise, it returns `None`
    pub fn follower(&self) -> Option<&'_ ValidatorDesc> {
        if self.channel.follower == self.validators.follower().id {
            Some(self.validators.follower())
        } else {
            None
        }
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
    use crate::BigNum;
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
    pub struct Pricing {
        pub min: BigNum,
        pub max: BigNum,
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
// TODO: Double check if we require all the methods and enums, as some parts are now in the `Campaign`
// This includes the matching of the Channel leader & follower to the Validators
pub mod validators {
    use crate::{ValidatorDesc, ValidatorId};
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
    /// A (leader, follower) tuple
    pub struct Validators(ValidatorDesc, ValidatorDesc);

    #[derive(Debug)]
    pub enum ValidatorRole<'a> {
        Leader(&'a ValidatorDesc),
        Follower(&'a ValidatorDesc),
    }

    impl<'a> ValidatorRole<'a> {
        pub fn validator(&self) -> &'a ValidatorDesc {
            match self {
                ValidatorRole::Leader(validator) => validator,
                ValidatorRole::Follower(validator) => validator,
            }
        }
    }

    impl Validators {
        pub fn new(leader: ValidatorDesc, follower: ValidatorDesc) -> Self {
            Self(leader, follower)
        }

        pub fn leader(&self) -> &ValidatorDesc {
            &self.0
        }

        pub fn follower(&self) -> &ValidatorDesc {
            &self.1
        }

        pub fn find(&self, validator_id: &ValidatorId) -> Option<ValidatorRole<'_>> {
            if &self.leader().id == validator_id {
                Some(ValidatorRole::Leader(&self.leader()))
            } else if &self.follower().id == validator_id {
                Some(ValidatorRole::Follower(&self.follower()))
            } else {
                None
            }
        }

        pub fn find_index(&self, validator_id: &ValidatorId) -> Option<u32> {
            if &self.leader().id == validator_id {
                Some(0)
            } else if &self.follower().id == validator_id {
                Some(1)
            } else {
                None
            }
        }

        pub fn iter(&self) -> Iter<'_> {
            Iter::new(&self)
        }
    }

    impl From<(ValidatorDesc, ValidatorDesc)> for Validators {
        fn from((leader, follower): (ValidatorDesc, ValidatorDesc)) -> Self {
            Self(leader, follower)
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

                    Some(self.validators.leader())
                }
                1 => {
                    self.index += 1;

                    Some(self.validators.follower())
                }
                _ => None,
            }
        }
    }
}

// TODO: Postgres Campaign
// TODO: Postgres CampaignSpec
// TODO: Postgres Validators
