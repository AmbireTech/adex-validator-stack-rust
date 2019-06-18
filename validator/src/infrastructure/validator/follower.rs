use domain::Channel;

use crate::domain::validator::{Validator, ValidatorFuture};
use futures::future::FutureExt;

#[derive(Clone)]
pub struct Follower {}

impl Validator for Follower {
    fn tick(&self, _channel: Channel) -> ValidatorFuture<()> {
        futures::future::ok(()).boxed()
    }
}
