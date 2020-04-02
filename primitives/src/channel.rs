use std::error::Error;
use std::fmt;

use chrono::serde::{ts_milliseconds, ts_milliseconds_option, ts_seconds};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use serde_hex::{SerHex, StrictPfx};

use crate::big_num::BigNum;
use crate::sentry::Event;
use crate::{AdUnit, EventSubmission, TargetingTag, ValidatorDesc, ValidatorId};
use hex::{FromHex, FromHexError};
use std::ops::Deref;

#[derive(Serialize, Deserialize, PartialEq, Eq, Copy, Clone, Hash)]
#[serde(transparent)]
pub struct ChannelId(
    #[serde(
        deserialize_with = "channel_id_from_str",
        serialize_with = "SerHex::<StrictPfx>::serialize"
    )]
    [u8; 32],
);

impl fmt::Debug for ChannelId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ChannelId({})", self)
    }
}

fn channel_id_from_str<'de, D>(deserializer: D) -> Result<[u8; 32], D::Error>
where
    D: Deserializer<'de>,
{
    let channel_id = String::deserialize(deserializer)?;
    if channel_id.is_empty() || channel_id.len() != 66 {
        return Err(serde::de::Error::custom("invalid channel id".to_string()));
    }

    <[u8; 32] as FromHex>::from_hex(&channel_id[2..]).map_err(serde::de::Error::custom)
}

impl Deref for ChannelId {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
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

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Channel {
    pub id: ChannelId,
    pub creator: ValidatorId,
    pub deposit_asset: String,
    pub deposit_amount: BigNum,
    #[serde(with = "ts_seconds")]
    pub valid_until: DateTime<Utc>,
    pub spec: ChannelSpec,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Pricing {
    pub max: BigNum,
    pub min: BigNum,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "UPPERCASE")]
pub struct PricingBounds {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub impression: Option<Pricing>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub click: Option<Pricing>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ChannelSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub validators: SpecValidators,
    /// Maximum payment per impression
    pub max_per_impression: BigNum,
    /// Minimum payment offered per impression
    pub min_per_impression: BigNum,
    /// Event pricing bounds
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pricing_bounds: Option<PricingBounds>,
    /// An array of TargetingTag (optional)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub targeting: Vec<TargetingTag>,
    /// Minimum targeting score (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_targeting_score: Option<f64>,
    /// EventSubmission object, applies to event submission (POST /channel/:id/events)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_submission: Option<EventSubmission>,
    /// A millisecond timestamp of when the campaign was created
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "ts_milliseconds_option"
    )]
    pub created: Option<DateTime<Utc>>,
    /// A millisecond timestamp representing the time you want this campaign to become active (optional)
    /// Used by the AdViewManager
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub price_multiplication_rules: Vec<PriceMultiplicationRules>,
    pub price_dynamic_adjustment: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct PriceMultiplicationRules {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multiplier: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amount: Option<BigNum>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ev_type: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub publisher: Option<Vec<ValidatorId>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os_type: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub country: Option<Vec<String>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
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

#[cfg(feature = "postgres")]
pub mod postgres {
    use super::ChannelId;
    use super::{Channel, ChannelSpec};
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
                spec: row.get::<_, Json<ChannelSpec>>("spec").0,
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
