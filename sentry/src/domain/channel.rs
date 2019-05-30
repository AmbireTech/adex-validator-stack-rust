use std::convert::TryFrom;

use chrono::{DateTime, Utc};
use chrono::serde::{ts_milliseconds, ts_seconds};
use serde::{Deserialize, Serialize};
use serde_hex::{SerHex, StrictPfx};

use crate::domain::{AdUnit, Asset, DomainError, EventSubmission, RepositoryFuture, TargetingTag, ValidatorDesc};
use crate::domain::bignum::BigNum;
use crate::util::serde::ts_milliseconds_option;

#[derive(Serialize, Deserialize, PartialEq, Eq, Debug, Copy, Clone)]
#[serde(transparent)]
pub struct ChannelId {
    #[serde(with = "SerHex::<StrictPfx>")]
    pub id: [u8; 32],
}

impl TryFrom<&str> for ChannelId {
    type Error = DomainError;

    /// Tries to create a ChannelId from &str
    /// which should be 32 bytes length.
    ///
    /// Example:
    ///
    /// ```
    /// use std::convert::TryFrom;
    /// use sentry::domain::channel::ChannelId;
    ///
    /// let bytes: [u8; 32] = [49, 50, 51, 52, 53, 54, 55, 56, 57, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57, 48, 49, 50];
    ///
    /// assert_eq!(ChannelId { id: bytes }, ChannelId::try_from("12345678901234567890123456789012").unwrap())
    /// ```
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let bytes = value.as_bytes();
        if bytes.len() != 32 {
            return Err(DomainError::InvalidArgument("The value of the id should have exactly 32 bytes".to_string()));
        }
        let mut id = [0; 32];
        id.copy_from_slice(&bytes[..32]);

        Ok(Self { id })
    }
}

impl PartialEq<ChannelId> for &str {
    fn eq(&self, channel_id: &ChannelId) -> bool {
        self.as_bytes() == channel_id.id
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Channel {
    pub id: ChannelId,
    pub creator: String,
    pub deposit_asset: Asset,
    pub deposit_amount: BigNum,
    #[serde(with = "ts_seconds")]
    pub valid_until: DateTime<Utc>,
    pub spec: ChannelSpec,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ChannelSpec {
    // TODO: Add the rest of the fields Issue #24
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    // TODO: Make a custom ser/deser 2 validators(leader, follower) array
    pub validators: Vec<ValidatorDesc>,
    /// Maximum payment per impression
    pub max_per_impression: BigNum,
    /// Minimum payment offered per impression
    pub min_per_impression: BigNum,
    /// An array of TargetingTag (optional)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub targeting: Vec<TargetingTag>,
    /// Minimum targeting score (optional)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_targeting_score: Option<u64>,
    /// EventSubmission object, applies to event submission (POST /channel/:id/events)
    pub event_submission: EventSubmission,
    /// A millisecond timestamp of when the campaign was created
    #[serde(with = "ts_milliseconds")]
    pub created: DateTime<Utc>,
    /// A millisecond timestamp representing the time you want this campaign to become active (optional)
    /// Used by the AdViewManager
    #[serde(default, skip_serializing_if = "Option::is_none", with = "ts_milliseconds_option")]
    pub active_from: Option<DateTime<Utc>>,
    /// A random number to ensure the campaignSpec hash is unique
    pub nonce: BigNum,
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

pub struct ChannelListParams {
    /// page to show, should be >= 1
    pub page: u32,
    /// channels limit per page, should be >= 1
    pub limit: u32,
    /// filters `valid_until` to be `>= valid_until_ge`
    pub valid_until_ge: DateTime<Utc>,
    /// filters the channels containing a specific validator if provided
    // @TODO: use a ValidatorName struct, to have a better control of having a valid ValidatorName at this point
    pub validator: Option<String>,
    /// Ensures that this struct can only be created by calling `new()`
    _secret: (),
}

impl ChannelListParams {
    pub fn new(valid_until_ge: DateTime<Utc>, limit: u32, page: u32, validator: Option<String>) -> Result<Self, DomainError> {
        if page < 1 {
            return Err(DomainError::InvalidArgument("Page should be >= 1".to_string()));
        }

        if limit < 1 {
            return Err(DomainError::InvalidArgument("Limit should be >= 1".to_string()));
        }

        let validator = validator
            .and_then(|s| match s.is_empty() {
                true => None,
                false => Some(s),
            });

        Ok(Self {
            valid_until_ge,
            page,
            limit,
            validator,
            _secret: (),
        })
    }
}

pub trait ChannelRepository: Send + Sync {
    /// Returns a list of channels, based on the passed Parameters for this method
    fn list(&self, params: &ChannelListParams) -> RepositoryFuture<Vec<Channel>>;

    fn save(&self, channel: Channel) -> RepositoryFuture<()>;

    fn find(&self, channel_id: &ChannelId) -> RepositoryFuture<Option<Channel>>;
}

#[cfg(test)]
#[path = "./channel_fixtures.rs"]
pub(crate) mod fixtures;

#[cfg(test)]
#[path = "./channel_test.rs"]
mod test;