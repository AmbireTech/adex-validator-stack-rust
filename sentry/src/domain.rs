use std::error;
use std::fmt;

pub use bignum::BigNum;
pub use channel::{Channel, ChannelSpec};
pub use validator::ValidatorDesc;

mod bignum;
mod channel;
mod validator;

#[derive(Debug, )]
pub struct DomainError;

impl fmt::Display for DomainError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Domain error",)
    }
}

impl error::Error for DomainError {
    fn cause(&self) -> Option<&error::Error> {
        None
    }
}
