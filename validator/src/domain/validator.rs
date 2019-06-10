use futures::Future;
use std::pin::Pin;

pub type ValidatorFuture<T> = Pin<Box<dyn Future<Output = Result<T, ValidatorError>> + Send>>;

#[derive(Debug)]
pub enum ValidatorError {
    None,
}

pub trait Validator {
    fn tick() -> ValidatorFuture<()>;
}
