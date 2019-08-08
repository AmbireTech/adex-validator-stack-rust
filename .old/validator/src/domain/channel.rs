use domain::{Channel, RepositoryFuture, ValidatorId};

pub trait ChannelRepository: Send + Sync {
    /// Returns list of all channels, based on the passed validator identity
    fn all(&self, identity: &ValidatorId) -> RepositoryFuture<Vec<Channel>>;
}
