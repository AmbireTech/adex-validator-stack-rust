use std::convert::TryFrom;
use std::error::Error;
use std::iter::Sum;
use std::str::FromStr;

use derive_more::{Add, Div, Mul, Sub};
use num_bigint::BigUint;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Serialize, Deserialize, Debug, Clone, Mul, Div, Add, Sub, Ord)]
pub struct BigNum(
    #[serde(
        deserialize_with = "biguint_from_str",
        serialize_with = "biguint_to_str"
    )]
    BigUint,
);

impl BigNum {
    pub fn new(num: BigUint) -> Result<Self, super::DomainError> {
        Ok(Self(num))
    }
}

impl Sum<BigNum> for BigNum {
    fn sum<I: Iterator<Item = BigNum>>(iter: I) -> Self {
        let sum_uint = iter.map(|big_num| big_num.0).sum();

        Self(sum_uint)
    }
}

impl TryFrom<&str> for BigNum {
    type Error = super::DomainError;

    fn try_from(num: &str) -> Result<Self, Self::Error> {
        let big_uint = BigUint::from_str(&num)
            .map_err(|err| super::DomainError::InvalidArgument(err.description().to_string()))?;

        Ok(Self(big_uint))
    }
}

impl ToString for BigNum {
    fn to_string(&self) -> String {
        self.0.to_str_radix(10)
    }
}

impl TryFrom<u32> for BigNum {
    type Error = super::DomainError;

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        BigNum::new(BigUint::from(value))
    }
}

fn biguint_from_str<'de, D>(deserializer: D) -> Result<BigUint, D::Error>
where
    D: Deserializer<'de>,
{
    let num = String::deserialize(deserializer)?;
    Ok(BigUint::from_str(&num).map_err(serde::de::Error::custom)?)
}

fn biguint_to_str<S>(num: &BigUint, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&num.to_str_radix(10))
}
