use futures::future::FutureExt;
use primitives::validator::{Validator, ValidatorFuture};
use primitives::Channel;

#[derive(Clone)]
pub struct Leader {}

impl Validator for Leader {
    fn tick(&self, _channel: Channel) -> ValidatorFuture<()> {
        futures::future::ok(()).boxed()
    }
}
