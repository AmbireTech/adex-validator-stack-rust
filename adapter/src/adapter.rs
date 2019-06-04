use crate::sanity::SanityChecker;
use domain::Channel;

pub trait Adapter: SanityChecker {
    fn whoami(&self) -> &String;

    fn validate_channel(&self, channel: &Channel) -> bool {
        Self::check(&self.whoami(), &channel).is_ok()
    }
}
