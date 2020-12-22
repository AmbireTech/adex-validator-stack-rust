use std::error::Error;
use std::fmt;
use std::ops::Deref;
use std::str::FromStr;

use chrono::serde::{ts_milliseconds, ts_milliseconds_option, ts_seconds};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use serde_hex::{SerHex, StrictPfx};

use crate::{targeting::Rules, AdUnit, BigNum, EventSubmission, ValidatorDesc, ValidatorId};
use hex::{FromHex, FromHexError};

#[derive(Serialize, Deserialize, PartialEq, Eq, Copy, Clone, Hash)]
#[serde(transparent)]
pub struct ChannelId(
    #[serde(
        deserialize_with = "deserialize_channel_id",
        serialize_with = "SerHex::<StrictPfx>::serialize"
    )]
    [u8; 32],
);

impl fmt::Debug for ChannelId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ChannelId({})", self)
    }
}

fn deserialize_channel_id<'de, D>(deserializer: D) -> Result<[u8; 32], D::Error>
where
    D: Deserializer<'de>,
{
    let channel_id = String::deserialize(deserializer)?;
    validate_channel_id(&channel_id).map_err(serde::de::Error::custom)
}

fn validate_channel_id(s: &str) -> Result<[u8; 32], FromHexError> {
    // strip `0x` prefix
    let hex = s.strip_prefix("0x").unwrap_or(s);
    // FromHex will make sure to check the length and match it to 32 bytes
    <[u8; 32] as FromHex>::from_hex(hex)
}

impl Deref for ChannelId {
    type Target = [u8; 32];

    fn deref(&self) -> &[u8; 32] {
        &self.0
    }
}

impl From<[u8; 32]> for ChannelId {
    fn from(array: [u8; 32]) -> Self {
        Self(array)
    }
}

impl AsRef<[u8]> for ChannelId {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl FromHex for ChannelId {
    type Error = FromHexError;

    fn from_hex<T: AsRef<[u8]>>(hex: T) -> Result<Self, Self::Error> {
        let array = hex::FromHex::from_hex(hex)?;

        Ok(Self(array))
    }
}

impl fmt::Display for ChannelId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{}", hex::encode(self.0))
    }
}

impl FromStr for ChannelId {
    type Err = FromHexError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        validate_channel_id(s).map(ChannelId)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Channel {
    pub id: ChannelId,
    pub creator: ValidatorId,
    pub deposit_asset: String,
    pub deposit_amount: BigNum,
    #[serde(with = "ts_seconds")]
    pub valid_until: DateTime<Utc>,
    #[serde(default)]
    pub targeting_rules: Rules,
    pub spec: ChannelSpec,
    #[serde(default)]
    pub exhausted: Vec<bool>,
}

pub fn channel_exhausted(channel: &Channel) -> bool {
    channel.exhausted.len() == 2 && channel.exhausted.iter().all(|&x| x)
}

#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
pub struct Pricing {
    pub max: BigNum,
    pub min: BigNum,
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

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ChannelSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub validators: SpecValidators,
    /// Maximum payment per impression
    /// **OBSOLETE**, only used if `pricingBounds` is missing an `IMPRESSION` entry
    pub max_per_impression: BigNum,
    /// Minimum payment offered per impression
    /// **OBSOLETE**, only used if `pricingBounds` is missing an `IMPRESSION` entry
    pub min_per_impression: BigNum,
    /// Event pricing bounds
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pricing_bounds: Option<PricingBounds>,
    /// EventSubmission object, applies to event submission (POST /channel/:id/events)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_submission: Option<EventSubmission>,
    /// A millisecond timestamp of when the campaign was created
    #[serde(with = "ts_milliseconds")]
    pub created: DateTime<Utc>,
    /// A millisecond timestamp representing the time you want this campaign to become active (optional)
    /// Used by the AdViewManager & Targeting AIP#31
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "ts_milliseconds_option"
    )]
    pub active_from: Option<DateTime<Utc>>,
    /// A random number to ensure the campaignSpec hash is unique
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nonce: Option<BigNum>,
    /// A millisecond timestamp of when the campaign should enter a withdraw period
    /// (no longer accept any events other than CHANNEL_CLOSE)
    /// A sane value should be lower than channel.validUntil * 1000 and higher than created
    /// It's recommended to set this at least one month prior to channel.validUntil * 1000
    #[serde(with = "ts_milliseconds")]
    pub withdraw_period_start: DateTime<Utc>,
    /// An array of AdUnit (optional)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ad_units: Vec<AdUnit>,
    #[serde(default)]
    pub targeting_rules: Rules,
}

#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
/// A (leader, follower) tuple
pub struct SpecValidators(ValidatorDesc, ValidatorDesc);

#[derive(Debug)]
pub enum SpecValidator<'a> {
    Leader(&'a ValidatorDesc),
    Follower(&'a ValidatorDesc),
}

impl<'a> SpecValidator<'a> {
    pub fn validator(&self) -> &'a ValidatorDesc {
        match self {
            SpecValidator::Leader(validator) => validator,
            SpecValidator::Follower(validator) => validator,
        }
    }
}

impl SpecValidators {
    pub fn new(leader: ValidatorDesc, follower: ValidatorDesc) -> Self {
        Self(leader, follower)
    }

    pub fn leader(&self) -> &ValidatorDesc {
        &self.0
    }

    pub fn follower(&self) -> &ValidatorDesc {
        &self.1
    }

    pub fn find(&self, validator_id: &ValidatorId) -> Option<SpecValidator<'_>> {
        if &self.leader().id == validator_id {
            Some(SpecValidator::Leader(&self.leader()))
        } else if &self.follower().id == validator_id {
            Some(SpecValidator::Follower(&self.follower()))
        } else {
            None
        }
    }

    pub fn find_index(&self, validator_id: &ValidatorId) -> Option<i32> {
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

impl From<(ValidatorDesc, ValidatorDesc)> for SpecValidators {
    fn from((leader, follower): (ValidatorDesc, ValidatorDesc)) -> Self {
        Self(leader, follower)
    }
}

/// Fixed size iterator of 2, as we need an iterator in couple of occasions
impl<'a> IntoIterator for &'a SpecValidators {
    type Item = &'a ValidatorDesc;
    type IntoIter = Iter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

pub struct Iter<'a> {
    validators: &'a SpecValidators,
    index: u8,
}

impl<'a> Iter<'a> {
    fn new(validators: &'a SpecValidators) -> Self {
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

#[derive(Debug, PartialEq, Eq)]
pub enum ChannelError {
    InvalidArgument(String),
    /// When the Adapter address is not listed in the `channel.spec.validators`
    /// which in terms means, that the adapter shouldn't handle this Channel
    AdapterNotIncluded,
    /// when `channel.valid_until` has passed (< now), the channel should be handled
    InvalidValidUntil(String),
    UnlistedValidator,
    UnlistedCreator,
    UnlistedAsset,
    MinimumDepositNotMet,
    MinimumValidatorFeeNotMet,
    FeeConstraintViolated,
}

impl fmt::Display for ChannelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ChannelError::InvalidArgument(error) => write!(f, "{}", error),
            ChannelError::AdapterNotIncluded => write!(f, "channel is not validated by us"),
            ChannelError::InvalidValidUntil(error) => write!(f, "{}", error),
            ChannelError::UnlistedValidator => write!(f, "validators are not in the whitelist"),
            ChannelError::UnlistedCreator => write!(f, "channel.creator is not whitelisted"),
            ChannelError::UnlistedAsset => write!(f, "channel.depositAsset is not whitelisted"),
            ChannelError::MinimumDepositNotMet => {
                write!(f, "channel.depositAmount is less than MINIMAL_DEPOSIT")
            }
            ChannelError::MinimumValidatorFeeNotMet => {
                write!(f, "channel validator fee is less than MINIMAL_FEE")
            }
            ChannelError::FeeConstraintViolated => {
                write!(f, "total fees <= deposit: fee constraint violated")
            }
        }
    }
}

impl Error for ChannelError {
    fn cause(&self) -> Option<&dyn Error> {
        None
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_channel_id_() {
        let hex_string = "061d5e2a67d0a9a10f1c732bca12a676d83f79663a396f7d87b3e30b9b411088";
        let prefixed_string = format!("0x{}", hex_string);

        let expected_id = ChannelId([
            0x06, 0x1d, 0x5e, 0x2a, 0x67, 0xd0, 0xa9, 0xa1, 0x0f, 0x1c, 0x73, 0x2b, 0xca, 0x12,
            0xa6, 0x76, 0xd8, 0x3f, 0x79, 0x66, 0x3a, 0x39, 0x6f, 0x7d, 0x87, 0xb3, 0xe3, 0x0b,
            0x9b, 0x41, 0x10, 0x88,
        ]);

        assert_eq!(ChannelId::from_str(hex_string).unwrap(), expected_id);
        assert_eq!(ChannelId::from_str(&prefixed_string).unwrap(), expected_id);
        assert_eq!(ChannelId::from_hex(hex_string).unwrap(), expected_id);

        let hex_value = serde_json::Value::String(hex_string.to_string());
        let prefixed_value = serde_json::Value::String(prefixed_string.clone());

        // Deserialization from JSON
        let de_hex_json =
            serde_json::from_value::<ChannelId>(hex_value.clone()).expect("Should deserialize");
        let de_prefixed_json = serde_json::from_value::<ChannelId>(prefixed_value.clone())
            .expect("Should deserialize");

        assert_eq!(de_hex_json, expected_id);
        assert_eq!(de_prefixed_json, expected_id);

        // Serialization to JSON
        let actual_serialized = serde_json::to_value(expected_id).expect("Should Serialize");
        // we don't expect any capitalization
        assert_eq!(
            actual_serialized,
            serde_json::Value::String(prefixed_string)
        )
    }
}

#[cfg(feature = "postgres")]
pub mod postgres {
    use super::ChannelId;
    use super::{Channel, ChannelSpec};
    use crate::targeting::Rules;
    use bytes::BytesMut;
    use hex::FromHex;
    use postgres_types::{accepts, to_sql_checked, FromSql, IsNull, Json, ToSql, Type};
    use std::error::Error;
    use tokio_postgres::Row;

    impl From<&Row> for Channel {
        fn from(row: &Row) -> Self {
            Self {
                id: row.get("id"),
                creator: row.get("creator"),
                deposit_asset: row.get("deposit_asset"),
                deposit_amount: row.get("deposit_amount"),
                valid_until: row.get("valid_until"),
                targeting_rules: row.get::<_, Json<Rules>>("targeting_rules").0,
                spec: row.get::<_, Json<ChannelSpec>>("spec").0,
                exhausted: row.get("exhausted"),
            }
        }
    }

    impl<'a> FromSql<'a> for ChannelId {
        fn from_sql(ty: &Type, raw: &'a [u8]) -> Result<Self, Box<dyn Error + Sync + Send>> {
            let str_slice = <&str as FromSql>::from_sql(ty, raw)?;

            Ok(ChannelId::from_hex(&str_slice[2..])?)
        }

        accepts!(TEXT, VARCHAR);
    }

    impl From<&Row> for ChannelId {
        fn from(row: &Row) -> Self {
            row.get("id")
        }
    }

    impl ToSql for ChannelId {
        fn to_sql(
            &self,
            ty: &Type,
            w: &mut BytesMut,
        ) -> Result<IsNull, Box<dyn Error + Sync + Send>> {
            let string = format!("0x{}", hex::encode(self));

            <String as ToSql>::to_sql(&string, ty, w)
        }

        fn accepts(ty: &Type) -> bool {
            <String as ToSql>::accepts(ty)
        }

        to_sql_checked!();
    }

    impl ToSql for ChannelSpec {
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
