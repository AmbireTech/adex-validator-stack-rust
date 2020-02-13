use std::convert::TryFrom;
use std::error::Error;
use std::iter::Sum;
use std::ops::{Add, AddAssign, Div, Mul, Sub};
use std::str::FromStr;

use num::rational::Ratio;
use num::{BigUint, CheckedSub, Integer};
use num_derive::{Num, NumOps, One, Zero};
use num_traits::Pow;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// DAI has precision of 18 decimals
/// For CPM we have 3 decimals precision, but that's for 1000 (3 decimals more)
/// This in terms means we need 18 - (3 + 3) = 12 decimals precision
pub const GLOBAL_MULTIPLIER: Multiplier = Multiplier(12);

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
/// Multiplier - Pow of 10 (10**n)
pub struct Multiplier(u64);

impl Mul<PrecisionU64> for Multiplier {
    type Output = BigUint;

    fn mul(self, rhs: PrecisionU64) -> Self::Output {
        let real_multiplier = BigUint::from(10u8).pow(BigUint::from(GLOBAL_MULTIPLIER.0));

        real_multiplier * rhs.0
    }
}

impl Into<BigNum> for Multiplier {
    fn into(self) -> BigNum {
        BigNum(self.into())
    }
}

impl Into<BigUint> for Multiplier {
    fn into(self) -> BigUint {
        BigUint::from(10u8).pow(BigUint::from(self.0))
    }
}

///
// @TODO: (De)serialize
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrecisionU64(u64);

impl Into<BigNum> for PrecisionU64 {
    fn into(self) -> BigNum {
        BigNum(GLOBAL_MULTIPLIER * self)
    }
}

impl From<BigNum> for PrecisionU64 {
    fn from(bignum: BigNum) -> Self {
        let precision = bignum
            .div_floor(&GLOBAL_MULTIPLIER.into())
            .to_u64()
            .unwrap_or(0);

        Self(precision)
    }
}

#[derive(
    Serialize,
    Deserialize,
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    NumOps,
    One,
    Zero,
    Num,
    Default,
)]
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

    pub fn to_str_radix(&self, radix: u32) -> String {
        self.0.to_str_radix(radix)
    }
}

impl Integer for BigNum {
    fn div_floor(&self, other: &Self) -> Self {
        self.0.div_floor(&other.0).into()
    }

    fn mod_floor(&self, other: &Self) -> Self {
        self.0.mod_floor(&other.0).into()
    }

    fn gcd(&self, other: &Self) -> Self {
        self.0.gcd(&other.0).into()
    }

    fn lcm(&self, other: &Self) -> Self {
        self.0.lcm(&other.0).into()
    }

    fn divides(&self, other: &Self) -> bool {
        self.0.divides(&other.0)
    }

    fn is_multiple_of(&self, other: &Self) -> bool {
        self.0.is_multiple_of(&other.0)
    }

    fn is_even(&self) -> bool {
        self.0.is_even()
    }

    fn is_odd(&self) -> bool {
        !self.is_even()
    }

    fn div_rem(&self, other: &Self) -> (Self, Self) {
        let (quotient, remainder) = self.0.div_rem(&other.0);

        (quotient.into(), remainder.into())
    }
}

impl Add<&BigNum> for &BigNum {
    type Output = BigNum;

    fn add(self, rhs: &BigNum) -> Self::Output {
        let big_uint = &self.0 + &rhs.0;
        BigNum(big_uint)
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
        BigNum(big_uint)
    }
}

impl Div<&BigNum> for &BigNum {
    type Output = BigNum;

    fn div(self, rhs: &BigNum) -> Self::Output {
        let big_uint = &self.0 / &rhs.0;
        BigNum(big_uint)
    }
}

impl Div<&BigNum> for BigNum {
    type Output = BigNum;

    fn div(self, rhs: &BigNum) -> Self::Output {
        let big_uint = &self.0 / &rhs.0;
        BigNum(big_uint)
    }
}

impl Mul<&BigNum> for &BigNum {
    type Output = BigNum;

    fn mul(self, rhs: &BigNum) -> Self::Output {
        let big_uint = &self.0 * &rhs.0;
        BigNum(big_uint)
    }
}

impl Mul<&BigNum> for BigNum {
    type Output = BigNum;

    fn mul(self, rhs: &BigNum) -> Self::Output {
        let big_uint = &self.0 * &rhs.0;
        BigNum(big_uint)
    }
}

impl<'a> Sum<&'a BigNum> for BigNum {
    fn sum<I: Iterator<Item = &'a BigNum>>(iter: I) -> Self {
        let sum_uint = iter.map(|big_num| &big_num.0).sum();

        Self(sum_uint)
    }
}

impl CheckedSub for BigNum {
    fn checked_sub(&self, v: &Self) -> Option<Self> {
        self.0.checked_sub(&v.0).map(Self)
    }
}

impl Mul<&Ratio<BigNum>> for &BigNum {
    type Output = BigNum;

    fn mul(self, rhs: &Ratio<BigNum>) -> Self::Output {
        // perform multiplication first!
        (self * rhs.numer()) / rhs.denom()
    }
}

impl Mul<&Ratio<BigNum>> for BigNum {
    type Output = BigNum;

    fn mul(self, rhs: &Ratio<BigNum>) -> Self::Output {
        // perform multiplication first!
        (self * rhs.numer()) / rhs.denom()
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
        Self(BigUint::from(value))
    }
}

impl From<BigUint> for BigNum {
    fn from(value: BigUint) -> Self {
        Self(value)
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

#[cfg(feature = "postgres")]
pub mod postgres {
    use super::BigNum;
    use bytes::BytesMut;
    use postgres_types::{FromSql, IsNull, ToSql, Type};
    use std::error::Error;

    impl<'a> FromSql<'a> for BigNum {
        fn from_sql(ty: &Type, raw: &'a [u8]) -> Result<BigNum, Box<dyn Error + Sync + Send>> {
            use std::convert::TryInto;

            let str_slice = <&str as FromSql>::from_sql(ty, raw)?;

            Ok(str_slice.try_into()?)
        }

        fn accepts(ty: &Type) -> bool {
            match *ty {
                Type::TEXT | Type::VARCHAR => true,
                _ => false,
            }
        }
    }

    impl ToSql for BigNum {
        fn to_sql(
            &self,
            ty: &Type,
            w: &mut BytesMut,
        ) -> Result<IsNull, Box<dyn Error + Sync + Send>> {
            <String as ToSql>::to_sql(&self.0.to_string(), ty, w)
        }

        fn accepts(ty: &Type) -> bool {
            match *ty {
                Type::TEXT | Type::VARCHAR => true,
                _ => false,
            }
        }

        fn to_sql_checked(
            &self,
            ty: &Type,
            out: &mut BytesMut,
        ) -> Result<IsNull, Box<dyn Error + Sync + Send>> {
            <String as ToSql>::to_sql_checked(&self.0.to_string(), ty, out)
        }
    }
}
#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn bignum_mul_by_ratio() {
        let big_num: BigNum = 50.into();
        let ratio: Ratio<BigNum> = (23.into(), 100.into()).into();

        let expected: BigNum = 11.into();
        assert_eq!(expected, &big_num * &ratio);
    }

    #[test]
    fn precision_u64_to_bignum() {
        let precision = PrecisionU64(5);
        let bignum = precision.into();

        assert_eq!(BigNum::from(5_000_000_000_000), bignum)
    }

    #[test]
    fn bignum_to_precision_u64() {
        // less than the multiplier 12
        let zero_bignum = BigNum::from(900_000_000_000);
        // it should floor to 0
        assert_eq!(PrecisionU64(0), PrecisionU64::from(zero_bignum));

        let bignum = BigNum::from(5_000_000_000_000);

        assert_eq!(PrecisionU64(5), PrecisionU64::from(bignum))
    }
}
