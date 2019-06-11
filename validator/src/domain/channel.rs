use domain::{BigNum, Channel, ChannelId, RepositoryFuture};

pub trait ChannelRepository: Send + Sync {
    /// Returns list of all channels, based on the passed validator identity
    fn all(&self, identity: &str) -> RepositoryFuture<Vec<Channel>>;
}
