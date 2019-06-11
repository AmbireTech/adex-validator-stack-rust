use domain::{BigNum, Channel, ChannelId, RepositoryFuture};

pub trait ChannelRepository: Send + Sync {
    /// Returns list of all channels, based on the passed Parameters for this method
    fn all(&self, channel_id: &ChannelId) -> RepositoryFuture<Vec<Channel>>;
}
