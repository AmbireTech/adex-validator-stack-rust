use crate::domain::validator::{Validator, ValidatorFuture};

#[derive(Clone)]
pub struct Leader {}

impl Validator for Leader {
    fn tick() -> ValidatorFuture<()> {
        unimplemented!()
    }
}
