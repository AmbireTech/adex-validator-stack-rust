use std::error::Error;
use std::fmt;

use chrono::serde::{ts_milliseconds, ts_milliseconds_option, ts_seconds};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_hex::{SerHex, StrictPfx};

use crate::big_num::BigNum;
use crate::{AdUnit, EventSubmission, TargetingTag, ValidatorDesc, ValidatorId};
use hex::{FromHex, FromHexError};
use std::ops::Deref;

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Copy, Clone)]
#[serde(transparent)]
pub struct ChannelId(#[serde(with = "SerHex::<StrictPfx>")] [u8; 32]);

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

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Channel {
    pub id: ChannelId,
    pub creator: String,
    pub deposit_asset: String,
    pub deposit_amount: BigNum,
    #[serde(with = "ts_seconds")]
    pub valid_until: DateTime<Utc>,
    pub spec: ChannelSpec,
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
    /// An array of TargetingTag (optional)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub targeting: Vec<TargetingTag>,
    /// Minimum targeting score (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_targeting_score: Option<f64>,
    /// EventSubmission object, applies to event submission (POST /channel/:id/events)
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
}

#[derive(Serialize, Deserialize, Debug, Clone)]
/// A (leader, follower) tuple
pub struct SpecValidators(ValidatorDesc, ValidatorDesc);

pub enum SpecValidator<'a> {
    Leader(&'a ValidatorDesc),
    Follower(&'a ValidatorDesc),
    None,
}

impl<'a> SpecValidator<'a> {
    pub fn is_some(&self) -> bool {
        match &self {
            SpecValidator::None => false,
            _ => true,
        }
    }

    pub fn is_none(&self) -> bool {
        !self.is_some()
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

    pub fn find(&self, validator_id: &ValidatorId) -> SpecValidator<'_> {
        if &self.leader().id == validator_id {
            SpecValidator::Leader(&self.leader())
        } else if &self.follower().id == validator_id {
            SpecValidator::Follower(&self.follower())
        } else {
            SpecValidator::None
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
    PassedValidUntil,
    UnlistedValidator,
    UnlistedCreator,
    UnlistedAsset,
    MinimumDepositNotMet,
    MinimumValidatorFeeNotMet,
}

impl fmt::Display for ChannelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Channel error",)
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
