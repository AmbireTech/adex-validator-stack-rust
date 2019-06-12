use crate::domain::validator::{Validator, ValidatorFuture};

pub struct Leader {}

impl Validator for Leader {
    fn tick() -> ValidatorFuture<()> {
        unimplemented!()
    }
}
