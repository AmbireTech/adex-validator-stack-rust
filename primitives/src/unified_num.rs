use num::{pow::Pow, CheckedSub, Integer, One};
use num_derive::{Num, NumOps, Zero};
use std::{
    cmp::Ordering,
    fmt,
    iter::Sum,
    ops::{Add, AddAssign, Div, Mul, Sub},
};

use crate::BigNum;

/// Unified precision Number with precision 8
#[derive(Num, NumOps, Zero, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct UnifiedNum(BigNum);

impl UnifiedNum {
    pub const PRECISION: u8 = 8;

    pub fn div_floor(&self, other: &Self) -> Self {
        Self(self.0.div_floor(&other.0))
    }

    pub fn to_f64(&self) -> Option<f64> {
        self.0.to_f64()
    }

    pub fn to_u64(&self) -> Option<u64> {
        self.0.to_u64()
    }

    /// Transform the UnifiedNum precision 8 to a new precision
    pub fn to_precision(&self, precision: u8) -> BigNum {
        match precision.cmp(&Self::PRECISION) {
            Ordering::Equal => self.0.clone(),
            Ordering::Less => self
                .0
                .div_floor(&BigNum::from(10).pow(Self::PRECISION - precision)),
            Ordering::Greater => (&self.0).mul(&BigNum::from(10).pow(precision - Self::PRECISION)),
        }
    }
}

impl From<u64> for UnifiedNum {
    fn from(number: u64) -> Self {
        Self(BigNum::from(number))
    }
}

impl From<BigNum> for UnifiedNum {
    fn from(number: BigNum) -> Self {
        Self(number)
    }
}

impl fmt::Display for UnifiedNum {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut string_value = self.0.to_str_radix(10);
        let value_length = string_value.len();
        let precision: usize = Self::PRECISION.into();

        if value_length > precision {
            string_value.insert_str(value_length - precision, ".");

            f.write_str(&string_value)
        } else {
            write!(f, "0.{:0>8}", string_value)
        }
    }
}

impl fmt::Debug for UnifiedNum {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "UnifiedNum({})", self.to_string())
    }
}

impl One for UnifiedNum {
    fn one() -> Self {
        Self(BigNum::from(10_000_000))
    }
}

impl Integer for UnifiedNum {
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

impl Pow<UnifiedNum> for UnifiedNum {
    type Output = UnifiedNum;

    fn pow(self, rhs: UnifiedNum) -> Self::Output {
        Self(self.0.pow(rhs.0))
    }
}

impl Pow<&UnifiedNum> for UnifiedNum {
    type Output = UnifiedNum;

    fn pow(self, rhs: &UnifiedNum) -> Self::Output {
        UnifiedNum(self.0.pow(&rhs.0))
    }
}

impl Pow<UnifiedNum> for &UnifiedNum {
    type Output = UnifiedNum;

    fn pow(self, rhs: UnifiedNum) -> Self::Output {
        UnifiedNum((&self.0).pow(rhs.0))
    }
}

impl Pow<&UnifiedNum> for &UnifiedNum {
    type Output = UnifiedNum;

    fn pow(self, rhs: &UnifiedNum) -> Self::Output {
        UnifiedNum((&self.0).pow(&rhs.0))
    }
}

impl Add<&UnifiedNum> for &UnifiedNum {
    type Output = UnifiedNum;

    fn add(self, rhs: &UnifiedNum) -> Self::Output {
        let bignum = &self.0 + &rhs.0;
        UnifiedNum(bignum)
    }
}

impl AddAssign<&UnifiedNum> for UnifiedNum {
    fn add_assign(&mut self, rhs: &UnifiedNum) {
        self.0 += &rhs.0
    }
}

impl Sub<&UnifiedNum> for &UnifiedNum {
    type Output = UnifiedNum;

    fn sub(self, rhs: &UnifiedNum) -> Self::Output {
        let bignum = &self.0 - &rhs.0;
        UnifiedNum(bignum)
    }
}

impl Sub<&UnifiedNum> for UnifiedNum {
    type Output = UnifiedNum;

    fn sub(self, rhs: &UnifiedNum) -> Self::Output {
        let bignum = &self.0 - &rhs.0;
        UnifiedNum(bignum)
    }
}

impl Sub<UnifiedNum> for &UnifiedNum {
    type Output = UnifiedNum;

    fn sub(self, rhs: UnifiedNum) -> Self::Output {
        let bignum = &self.0 - &rhs.0;
        UnifiedNum(bignum)
    }
}

impl Div<&UnifiedNum> for &UnifiedNum {
    type Output = UnifiedNum;

    fn div(self, rhs: &UnifiedNum) -> Self::Output {
        let bignum = &self.0 / &rhs.0;
        UnifiedNum(bignum)
    }
}

impl Div<&UnifiedNum> for UnifiedNum {
    type Output = UnifiedNum;

    fn div(self, rhs: &UnifiedNum) -> Self::Output {
        let bignum = &self.0 / &rhs.0;
        UnifiedNum(bignum)
    }
}

impl Mul<&UnifiedNum> for &UnifiedNum {
    type Output = UnifiedNum;

    fn mul(self, rhs: &UnifiedNum) -> Self::Output {
        let bignum = &self.0 * &rhs.0;
        UnifiedNum(bignum)
    }
}

impl Mul<&UnifiedNum> for UnifiedNum {
    type Output = UnifiedNum;

    fn mul(self, rhs: &UnifiedNum) -> Self::Output {
        let bignum = &self.0 * &rhs.0;
        UnifiedNum(bignum)
    }
}

impl<'a> Sum<&'a UnifiedNum> for UnifiedNum {
    fn sum<I: Iterator<Item = &'a UnifiedNum>>(iter: I) -> Self {
        let sum_uint = iter.map(|big_num| &big_num.0).sum();

        Self(sum_uint)
    }
}

impl CheckedSub for UnifiedNum {
    fn checked_sub(&self, v: &Self) -> Option<Self> {
        self.0.checked_sub(&v.0).map(Self)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn unified_num_displays_correctly() {
        let one = UnifiedNum::from(100_000_000);
        let zero_point_one = UnifiedNum::from(10_000_000);
        let smallest_value = UnifiedNum::from(1);
        let random_value = UnifiedNum::from(144_903_000_567_000);

        assert_eq!("1.00000000", &one.to_string());
        assert_eq!("0.10000000", &zero_point_one.to_string());
        assert_eq!("0.00000001", &smallest_value.to_string());
        assert_eq!("1449030.00567000", &random_value.to_string());
    }

    #[test]
    fn test_convert_unified_num_to_new_precision() {
        let dai_precision: u8 = 18;
        let usdt_precision: u8 = 6;
        let same_precision = UnifiedNum::PRECISION;

        let dai_power = BigNum::from(10).pow(BigNum::from(dai_precision as u64));

        // 321.00000000
        let dai_unified = UnifiedNum::from(32_100_000_000_u64);
        let dai_expected = BigNum::from(321_u64) * dai_power;
        assert_eq!(dai_expected, dai_unified.to_precision(dai_precision));

        // 321.00000777 - should floor to 321.000007 (precision 6)
        let usdt_unified = UnifiedNum::from(32_100_000_777_u64);
        let usdt_expected = BigNum::from(321_000_007_u64);
        assert_eq!(
            usdt_expected,
            usdt_unified.to_precision(usdt_precision),
            "It should floor the result of USDT"
        );

        // 321.00000999
        let same_unified = UnifiedNum::from(32_100_000_777_u64);
        assert_eq!(
            same_unified.0,
            same_unified.to_precision(same_precision),
            "It should not make any adjustments to the precision"
        );
    }
}
