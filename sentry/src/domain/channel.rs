use std::fmt::Debug;

use chrono::{DateTime, Utc};

use domain::{Channel, RepositoryFuture};
use domain::{ChannelId, DomainError};

pub struct ChannelListParams {
    /// page to show, should be >= 1
    pub page: u32,
    /// channels limit per page, should be >= 1
    pub limit: u32,
    /// filters `valid_until` to be `>= valid_until_ge`
    pub valid_until_ge: DateTime<Utc>,
    /// filters the channels containing a specific validator if provided
    pub validator: Option<String>,
    /// Ensures that this struct can only be created by calling `new()`
    _secret: (),
}

impl ChannelListParams {
    pub fn new(
        valid_until_ge: DateTime<Utc>,
        limit: u32,
        page: u32,
        validator: Option<String>,
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

        let validator = validator.and_then(|s| if s.is_empty() { None } else { Some(s) });

        Ok(Self {
            valid_until_ge,
            page,
            limit,
            validator,
            _secret: (),
        })
    }
}

pub trait ChannelRepository: Debug + Send + Sync {
    /// Returns a list of channels, based on the passed Parameters for this method
    fn list(&self, params: &ChannelListParams) -> RepositoryFuture<Vec<Channel>>;

    fn find(&self, channel_id: &ChannelId) -> RepositoryFuture<Option<Channel>>;

    fn create(&self, channel: Channel) -> RepositoryFuture<()>;
}
