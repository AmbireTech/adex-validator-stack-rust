use std::convert::TryFrom;
use std::error::Error;
use std::iter::Sum;
use std::ops::{Add, AddAssign, Div, Mul, Sub};
use std::str::FromStr;

use num_bigint::BigUint;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
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

    pub fn div_floor(&self, other: &Self) -> Self {
        use num::integer::Integer;

        Self(self.0.div_floor(&other.0))
    }

    pub fn to_f64(&self) -> Option<f64> {
        use num::traits::cast::ToPrimitive;

        self.0.to_f64()
    }

    pub fn to_u64(&self) -> Option<u64> {
        use num::traits::cast::ToPrimitive;

        self.0.to_u64()
    }
}

impl Add<&BigNum> for &BigNum {
    type Output = BigNum;

    fn add(self, rhs: &BigNum) -> Self::Output {
        let big_uint = &self.0 + &rhs.0;
        BigNum(big_uint.to_owned())
    }
}

impl AddAssign<&BigNum> for BigNum {
    fn add_assign(&mut self, rhs: &BigNum) {
        self.0 += &rhs.0
    }
}

impl Sub<&BigNum> for &BigNum {
    type Output = BigNum;

    fn sub(self, rhs: &BigNum) -> Self::Output {
        let big_uint = &self.0 - &rhs.0;
        BigNum(big_uint.to_owned())
    }
}

impl Div<&BigNum> for &BigNum {
    type Output = BigNum;

    fn div(self, rhs: &BigNum) -> Self::Output {
        let big_uint = &self.0 / &rhs.0;
        BigNum(big_uint.to_owned())
    }
}

impl Mul<&BigNum> for &BigNum {
    type Output = BigNum;

    fn mul(self, rhs: &BigNum) -> Self::Output {
        let big_uint = &self.0 * &rhs.0;
        BigNum(big_uint.to_owned())
    }
}

impl<'a> Sum<&'a BigNum> for BigNum {
    fn sum<I: Iterator<Item = &'a BigNum>>(iter: I) -> Self {
        let sum_uint = iter.map(|big_num| &big_num.0).sum();

        Self(sum_uint)
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

impl From<u64> for BigNum {
    fn from(value: u64) -> Self {
        BigNum(BigUint::from(value))
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
