use std::error::Error;
use std::fmt;

use chrono::serde::{ts_milliseconds, ts_seconds};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_hex::{SerHex, StrictPfx};

use crate::big_num::BigNum;
use crate::util::serde::ts_milliseconds_option;
use crate::{AdUnit, EventSubmission, TargetingTag, ValidatorDesc, ValidatorId};

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Channel {
    #[serde(with = "SerHex::<StrictPfx>")]
    pub id: [u8; 32],
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
#[serde(transparent)]
pub struct SpecValidators([ValidatorDesc; 2]);

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
        Self([leader, follower])
    }

    pub fn leader(&self) -> &ValidatorDesc {
        &self.0[0]
    }

    pub fn follower(&self) -> &ValidatorDesc {
        &self.0[1]
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
}

impl From<[ValidatorDesc; 2]> for SpecValidators {
    fn from(slice: [ValidatorDesc; 2]) -> Self {
        Self(slice)
    }
}

impl<'a> IntoIterator for &'a SpecValidators {
    type Item = &'a ValidatorDesc;
    type IntoIter = ::std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        vec![self.leader(), self.follower()].into_iter()
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
