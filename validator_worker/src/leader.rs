use primitives::validator::{Validator, ValidatorFuture};
use primitives::{Channel};
use futures::future::FutureExt;

#[derive(Clone)]
pub struct Leader {}

impl Validator for Leader {
    fn tick(&self, _channel: Channel) -> ValidatorFuture<()> {
        futures::future::ok(()).boxed()
    }
}
