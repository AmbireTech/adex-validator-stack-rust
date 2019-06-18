use crate::domain::validator::{Validator, ValidatorFuture};

#[derive(Clone)]
pub struct Follower {}

impl Validator for Follower {
    fn tick() -> ValidatorFuture<()> {
        unimplemented!()
    }
}
