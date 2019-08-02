use chrono::{DateTime, Utc};

use domain::{Channel, RepositoryFuture, ValidatorId};
use domain::{ChannelId, DomainError};
use std::convert::TryFrom;

pub struct ChannelListParams {
    /// page to show, should be >= 1
    pub page: u64,
    /// channels limit per page, should be >= 1
    pub limit: u32,
    /// filters `valid_until` to be `>= valid_until_ge`
    pub valid_until_ge: DateTime<Utc>,
    /// filters the channels containing a specific validator if provided
    pub validator: Option<ValidatorId>,
    /// Ensures that this struct can only be created by calling `new()`
    _secret: (),
}

impl ChannelListParams {
    pub fn new(
        valid_until_ge: DateTime<Utc>,
        limit: u32,
        page: u64,
        validator: Option<&str>,
    ) -> Result<Self, DomainError> {
        if page < 1 {
            return Err(DomainError::InvalidArgument(
                "Page should be >= 1".to_string(),
            ));
        }

        if limit < 1 {
            return Err(DomainError::InvalidArgument(
                "Limit should be >= 1".to_string(),
            ));
        }

        let validator = validator
            .and_then(|s| if s.is_empty() { None } else { Some(s) })
            .map(ValidatorId::try_from)
            .transpose()?;

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

    fn list_count(&self, params: &ChannelListParams) -> RepositoryFuture<u64>;

    fn find(&self, channel_id: &ChannelId) -> RepositoryFuture<Option<Channel>>;

    fn add(&self, channel: Channel) -> RepositoryFuture<()>;
}
