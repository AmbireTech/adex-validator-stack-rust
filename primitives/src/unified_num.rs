use crate::BigNum;
use num::{
    pow::Pow, traits::CheckedRem, CheckedAdd, CheckedDiv, CheckedMul, CheckedSub, Integer, One,
};
use num_derive::{FromPrimitive, Num, NumCast, NumOps, ToPrimitive, Zero};
use serde::{Deserialize, Serialize};
use std::{
    cmp::Ordering,
    fmt,
    iter::Sum,
    ops::{Add, AddAssign, Div, Mul, Sub},
};

/// Unified Number with a precision of 8 digits after the decimal point.
///
/// The number can be a maximum of `u64::MAX` (the underlying type),
/// or in a `UnifiedNum` value `184_467_440_737.09551615`.
/// The actual number is handled as a unsigned number and only the display shows the decimal point.
///
/// This number is (de)serialized as a Javascript number which is `f64`.
/// As far as the numbers don't exceed `2**63`, the Javascript number should be sufficient without losing precision
#[derive(
    Clone,
    Copy,
    Num,
    NumOps,
    NumCast,
    ToPrimitive,
    FromPrimitive,
    Zero,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
)]
#[serde(transparent)]
pub struct UnifiedNum(u64);

impl UnifiedNum {
    pub const PRECISION: u8 = 8;

    pub fn div_floor(&self, other: &Self) -> Self {
        Self(self.0.div_floor(&other.0))
    }

    pub const fn from_u64(value: u64) -> Self {
        Self(value)
    }

    pub const fn to_u64(&self) -> u64 {
        self.0
    }

    pub fn to_bignum(&self) -> BigNum {
        BigNum::from(self.0)
    }

    pub fn checked_add(&self, rhs: &UnifiedNum) -> Option<Self> {
        CheckedAdd::checked_add(self, rhs)
    }

    pub fn checked_sub(&self, rhs: &UnifiedNum) -> Option<Self> {
        CheckedSub::checked_sub(self, rhs)
    }

    pub fn checked_mul(&self, rhs: &UnifiedNum) -> Option<Self> {
        CheckedMul::checked_mul(self, rhs)
    }

    pub fn checked_div(&self, rhs: &UnifiedNum) -> Option<Self> {
        CheckedDiv::checked_div(self, rhs)
    }

    pub fn checked_rem(&self, rhs: &UnifiedNum) -> Option<Self> {
        CheckedRem::checked_rem(self, rhs)
    }

    /// Transform the UnifiedNum precision 8 to a new precision
    pub fn to_precision(&self, precision: u8) -> BigNum {
        let inner = BigNum::from(self.0);
        match precision.cmp(&Self::PRECISION) {
            Ordering::Equal => inner,
            Ordering::Less => inner.div_floor(&BigNum::from(10).pow(Self::PRECISION - precision)),
            Ordering::Greater => inner.mul(&BigNum::from(10).pow(precision - Self::PRECISION)),
        }
    }
}

impl From<u64> for UnifiedNum {
    fn from(number: u64) -> Self {
        Self(number)
    }
}

impl fmt::Display for UnifiedNum {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut string_value = self.0.to_string();
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
        Self(100_000_000)
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
        UnifiedNum(self.0 + rhs.0)
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
        UnifiedNum(self.0 - rhs.0)
    }
}

impl Sub<&UnifiedNum> for UnifiedNum {
    type Output = UnifiedNum;

    fn sub(self, rhs: &UnifiedNum) -> Self::Output {
        UnifiedNum(self.0 - rhs.0)
    }
}

impl Sub<UnifiedNum> for &UnifiedNum {
    type Output = UnifiedNum;

    fn sub(self, rhs: UnifiedNum) -> Self::Output {
        UnifiedNum(self.0 - rhs.0)
    }
}

impl Div<&UnifiedNum> for &UnifiedNum {
    type Output = UnifiedNum;

    fn div(self, rhs: &UnifiedNum) -> Self::Output {
        UnifiedNum(self.0 / rhs.0)
    }
}

impl Div<&UnifiedNum> for UnifiedNum {
    type Output = UnifiedNum;

    fn div(self, rhs: &UnifiedNum) -> Self::Output {
        UnifiedNum(self.0 / rhs.0)
    }
}

impl Mul<&UnifiedNum> for &UnifiedNum {
    type Output = UnifiedNum;

    fn mul(self, rhs: &UnifiedNum) -> Self::Output {
        UnifiedNum(self.0 * rhs.0)
    }
}

impl Mul<&UnifiedNum> for UnifiedNum {
    type Output = UnifiedNum;

    fn mul(self, rhs: &UnifiedNum) -> Self::Output {
        UnifiedNum(self.0 * rhs.0)
    }
}

impl<'a> Sum<&'a UnifiedNum> for Option<UnifiedNum> {
    fn sum<I: Iterator<Item = &'a UnifiedNum>>(mut iter: I) -> Self {
        iter.try_fold(0_u64, |acc, unified| acc.checked_add(unified.0))
            .map(UnifiedNum)
    }
}

impl CheckedAdd for UnifiedNum {
    fn checked_add(&self, v: &Self) -> Option<Self> {
        self.0.checked_add(v.0).map(Self)
    }
}

impl CheckedSub for UnifiedNum {
    fn checked_sub(&self, v: &Self) -> Option<Self> {
        self.0.checked_sub(v.0).map(Self)
    }
}

impl CheckedMul for UnifiedNum {
    fn checked_mul(&self, v: &Self) -> Option<Self> {
        self.0.checked_mul(v.0).map(Self)
    }
}

impl CheckedDiv for UnifiedNum {
    fn checked_div(&self, v: &Self) -> Option<Self> {
        self.0.checked_div(v.0).map(Self)
    }
}

impl CheckedRem for UnifiedNum {
    fn checked_rem(&self, v: &Self) -> Option<Self> {
        self.0.checked_rem(v.0).map(Self)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use num::Zero;

    #[test]
    fn unified_num_sum() {
        let num_max = UnifiedNum(u64::MAX);
        let num_1 = UnifiedNum(1);
        let num_5 = UnifiedNum(5);

        let succeeding_sum: Option<UnifiedNum> = [num_1, num_5].iter().sum();
        let overflow_sum: Option<UnifiedNum> = [num_1, num_max].iter().sum();

        assert_eq!(Some(UnifiedNum(6)), succeeding_sum);
        assert_eq!(None, overflow_sum);
    }

    #[test]
    fn unified_num_displays_and_de_serializes_correctly() {
        let one = UnifiedNum::from(100_000_000);
        let zero_point_one = UnifiedNum::from(10_000_000);
        let smallest_value = UnifiedNum::from(1);
        let random_value = UnifiedNum::from(144_903_000_567_000);

        assert_eq!("1.00000000", &one.to_string());
        assert_eq!("1.00000000", &UnifiedNum::one().to_string());
        assert_eq!("0.00000000", &UnifiedNum::zero().to_string());
        assert_eq!("0.10000000", &zero_point_one.to_string());
        assert_eq!("0.00000001", &smallest_value.to_string());
        assert_eq!("1449030.00567000", &random_value.to_string());

        assert_eq!(
            serde_json::Value::Number(100_000_000.into()),
            serde_json::to_value(one).expect("Should serialize")
        )
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

        // 321.00000777
        let same_unified = UnifiedNum::from(32_100_000_777_u64);
        assert_eq!(
            BigNum::from(same_unified.0),
            same_unified.to_precision(same_precision),
            "It should not make any adjustments to the precision"
        );
    }

    #[test]
    fn div_and_floor_fee_calculation() {
        // 1.00007777
        let one_sevens = UnifiedNum::from(100_007_777_u64);
        let pro_milles = UnifiedNum::from(1_000);
        let division = one_sevens.div(&pro_milles);
        let fee = UnifiedNum::from(7);

        assert_eq!(UnifiedNum::from(100_007), division);
        // e.g. fee of 7 pro milles
        assert_eq!(UnifiedNum::from(700_049), division * &fee);
    }

    #[test]
    fn mul_first_and_div_fee_calculation() {
        // 1.00007777
        let one_sevens = UnifiedNum::from(100_007_777_u64);
        let pro_milles = UnifiedNum::from(1_000);
        let fee = UnifiedNum::from(7);
        let multiply = one_sevens.mul(&fee);

        // assert_eq!(UnifiedNum::from(100_007), multiply);
        // e.g. fee of 7 pro milles
        assert_eq!(UnifiedNum::from(700_049), multiply.div(&pro_milles));
    }

    #[test]
    fn div_rem_fee_calculation() {
        // 1.00007777
        let one_sevens = UnifiedNum::from(100_007_777_u64);
        let pro_milles = UnifiedNum::from(1_000);
        let fee = UnifiedNum::from(7);

        let (quotient, remainder) = one_sevens.div_rem(&pro_milles);
        let main_fee = quotient * &fee;
        assert_eq!(&UnifiedNum::from(700_049), &main_fee);

        let expected_remainder = UnifiedNum::from(777);
        assert_eq!(&expected_remainder, &remainder);

        let expected_fee_of_remainder = UnifiedNum::from(5_439).div_floor(&pro_milles);
        assert_eq!(
            expected_fee_of_remainder,
            (&expected_remainder * &fee).div_floor(&pro_milles)
        );

        assert_eq!(
            UnifiedNum::from(700_054),
            main_fee + expected_fee_of_remainder
        );
    }
}
