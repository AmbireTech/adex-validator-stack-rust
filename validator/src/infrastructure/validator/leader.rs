use domain::Channel;

use crate::domain::validator::{Validator, ValidatorFuture};
use futures::future::FutureExt;

#[derive(Clone)]
pub struct Leader {}

impl Validator for Leader {
    fn tick(&self, _channel: Channel) -> ValidatorFuture<()> {
        futures::future::ok(()).boxed()
    }
}
