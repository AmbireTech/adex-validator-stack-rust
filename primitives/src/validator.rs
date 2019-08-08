use std::convert::TryFrom;
use std::fmt;
//
use serde::{Deserialize, Serialize};
use crate::{ BigNum };
use std::pin::Pin;
use futures::Future;
use crate::Channel;

pub type ValidatorFuture<T> = Pin<Box<dyn Future<Output = Result<T, ValidatorError>> + Send>>;

#[derive(Debug)]
pub enum ValidatorError {
    None,
    InvalidRootHash,
    InvalidSignature,
    InvalidTransition
}

pub trait Validator {
    fn tick(&self, channel: Channel) -> ValidatorFuture<()>;
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ValidatorDesc {
    // @TODO: Replace id `String` with `ValidatorId` https://github.com/AdExNetwork/adex-validator-stack-rust/issues/83
    pub id: String,
    pub url: String,
    pub fee: BigNum,
}