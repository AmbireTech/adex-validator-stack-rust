use futures::future::FutureExt;
use primitives::validator::{Validator, ValidatorFuture};
use primitives::Channel;

#[derive(Clone)]
pub struct Follower {}

impl Validator for Follower {
    fn tick(&self, _channel: Channel) -> ValidatorFuture<()> {
        futures::future::ok(()).boxed()
    }
}
