use std::convert::TryFrom;
use std::fmt;
use std::iter::Sum;
use std::ops::{Add, AddAssign, Div, Mul, Sub};
use std::str::FromStr;

use num::rational::Ratio;
use num::{BigUint, CheckedSub, Integer};
use num_derive::{Num, NumOps, One, Zero};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(
    Serialize, Deserialize, Clone, PartialEq, Eq, PartialOrd, Ord, NumOps, One, Zero, Num, Default,
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

    pub fn from_bytes_be(buf: &[u8]) -> Self {
        Self(BigUint::from_bytes_be(buf))
    }
}

impl fmt::Debug for BigNum {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let radix = 10;
        let value = self.to_str_radix(radix);
        write!(f, "BigNum(radix: {}; {})", radix, value)
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
            .map_err(|err| super::DomainError::InvalidArgument(err.to_string()))?;

        Ok(Self(big_uint))
    }
}

impl FromStr for BigNum {
    type Err = super::DomainError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        BigNum::try_from(s)
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
            matches!(*ty, Type::TEXT | Type::VARCHAR)
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
            matches!(*ty, Type::TEXT | Type::VARCHAR)
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
}
