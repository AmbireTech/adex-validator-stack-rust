use num::{CheckedSub, Integer, One};
use num_derive::{Num, NumOps, Zero};
use std::{
    fmt,
    iter::Sum,
    ops::{Add, AddAssign, Div, Mul, Sub},
};

use crate::BigNum;

/// Unified precision Number with precision 8
#[derive(Num, NumOps, Zero, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct UnifiedNum(BigNum);

impl UnifiedNum {
    pub const PRECISION: usize = 8;

    pub fn div_floor(&self, other: &Self) -> Self {
        Self(self.0.div_floor(&other.0))
    }

    pub fn to_f64(&self) -> Option<f64> {
        self.0.to_f64()
    }

    pub fn to_u64(&self) -> Option<u64> {
        self.0.to_u64()
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

        if value_length > Self::PRECISION {
            string_value.insert_str(value_length - Self::PRECISION, ".");

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
    use crate::UnifiedNum;

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
}
