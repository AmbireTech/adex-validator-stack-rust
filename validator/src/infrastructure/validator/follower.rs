use crate::domain::validator::{Validator, ValidatorFuture};

pub struct Follower {}

impl Validator for Follower {
    fn tick() -> ValidatorFuture<()> {
        unimplemented!()
    }
}
