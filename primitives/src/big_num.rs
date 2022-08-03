use std::{
    fmt,
    iter::Sum,
    ops::{Add, AddAssign, Div, Mul, Sub},
    str::FromStr,
};

use num::{pow::Pow, rational::Ratio, BigUint, CheckedSub, Integer};
use num_derive::{Num, NumOps, One, Zero};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::UnifiedNum;

/// Re-export of the [`num::bigint::ParseBigIntError`] when using [`BigNum`]
pub use num::bigint::ParseBigIntError;
#[derive(
    Serialize,
    Deserialize,
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
    Hash,
)]
pub struct BigNum(
    #[serde(
        deserialize_with = "biguint_from_str",
        serialize_with = "biguint_to_str"
    )]
    BigUint,
);

impl BigNum {
    pub fn new(num: BigUint) -> Result<Self, ParseBigIntError> {
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

    /// With this method you can easily create a [`BigNum`] from a whole number
    ///
    /// # Example
    ///
    /// ```
    /// # use primitives::BigNum;
    /// let dai_precision = 18;
    /// let whole_number = 15;
    ///
    /// let bignum = BigNum::with_precision(whole_number, dai_precision);
    /// let expected = "15000000000000000000";
    ///
    /// assert_eq!(expected, &bignum.to_string());
    /// ```
    pub fn with_precision(whole_number: u64, with_precision: u8) -> Self {
        let multiplier = 10_u64.pow(with_precision.into());

        BigNum::from(whole_number).mul(&multiplier)
    }
}

impl fmt::Debug for BigNum {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let radix = 10;
        let value = self.to_str_radix(radix);
        write!(f, "BigNum(radix: {}; {})", radix, value)
    }
}

impl fmt::Display for BigNum {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
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

impl Pow<BigNum> for BigNum {
    type Output = BigNum;

    fn pow(self, rhs: BigNum) -> Self::Output {
        Self(self.0.pow(rhs.0))
    }
}

impl Pow<&BigNum> for BigNum {
    type Output = BigNum;

    fn pow(self, rhs: &BigNum) -> Self::Output {
        BigNum(self.0.pow(&rhs.0))
    }
}

impl Pow<BigNum> for &BigNum {
    type Output = BigNum;

    fn pow(self, rhs: BigNum) -> Self::Output {
        BigNum(Pow::pow(&self.0, rhs.0))
    }
}

impl Pow<&BigNum> for &BigNum {
    type Output = BigNum;

    fn pow(self, rhs: &BigNum) -> Self::Output {
        BigNum(Pow::pow(&self.0, &rhs.0))
    }
}

impl Pow<u8> for BigNum {
    type Output = BigNum;

    fn pow(self, rhs: u8) -> Self::Output {
        BigNum(self.0.pow(rhs))
    }
}

impl Add<&BigNum> for BigNum {
    type Output = BigNum;

    fn add(self, rhs: &BigNum) -> Self::Output {
        let big_uint = &self.0 + &rhs.0;
        BigNum(big_uint)
    }
}

impl Add<&BigNum> for &BigNum {
    type Output = BigNum;

    fn add(self, rhs: &BigNum) -> Self::Output {
        let big_uint = &self.0 + &rhs.0;
        BigNum(big_uint)
    }
}

impl Add<BigNum> for &BigNum {
    type Output = BigNum;

    fn add(self, rhs: BigNum) -> Self::Output {
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

impl Mul<&u64> for BigNum {
    type Output = BigNum;

    fn mul(self, rhs: &u64) -> Self::Output {
        let big_uint = &self.0 * rhs;
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
    type Error = ParseBigIntError;

    fn try_from(num: &str) -> Result<Self, Self::Error> {
        BigUint::from_str(num).map(Self)
    }
}

impl FromStr for BigNum {
    type Err = ParseBigIntError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        BigNum::try_from(s)
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

impl<'a> Sum<&'a UnifiedNum> for BigNum {
    fn sum<I: Iterator<Item = &'a UnifiedNum>>(iter: I) -> BigNum {
        BigNum(iter.map(|unified| BigUint::from(unified.to_u64())).sum())
    }
}

fn biguint_from_str<'de, D>(deserializer: D) -> Result<BigUint, D::Error>
where
    D: Deserializer<'de>,
{
    let num = String::deserialize(deserializer)?;
    BigUint::from_str(&num).map_err(serde::de::Error::custom)
}

fn biguint_to_str<S>(num: &BigUint, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(&num.to_str_radix(10))
}

#[cfg(feature = "postgres")]
mod postgres {
    use super::BigNum;
    use bytes::BytesMut;
    use std::error::Error;
    use tokio_postgres::types::{FromSql, IsNull, ToSql, Type};

    impl<'a> FromSql<'a> for BigNum {
        fn from_sql(ty: &Type, raw: &'a [u8]) -> Result<BigNum, Box<dyn Error + Sync + Send>> {
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
    #[test]
    fn bignum_formatting() {
        let bignum: BigNum = 5000.into();

        assert_eq!("5000", &bignum.to_string());
        assert_eq!("BigNum(radix: 10; 5000)", &format!("{:?}", &bignum));
    }
}
