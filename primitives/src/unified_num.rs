use crate::BigNum;
use num::{
    pow::Pow, rational::Ratio, traits::CheckedRem, CheckedAdd, CheckedDiv, CheckedMul, CheckedSub,
    Integer, One,
};
use num_derive::{FromPrimitive, Num, ToPrimitive, Zero};
use parse_display::{Display, FromStr, ParseError};
use serde::{Deserialize, Serialize};
use std::{
    cmp::Ordering,
    fmt,
    iter::Sum,
    ops::{Add, AddAssign, Div, Mul, Rem, Sub},
};

pub use whole_number::FromWhole;

mod whole_number {
    use num::ToPrimitive;

    use crate::UnifiedNum;

    /// Helper trait for handling the creation of special numbers from a whole number
    pub trait FromWhole<T>: Sized {
        /// # Panics
        /// If the number is greater than one can be represented.
        fn from_whole(whole_number: T) -> Self;

        /// Same as [`Self::from_whole`] but instead of panicking it returns an Option.
        fn from_whole_opt(whole_number: T) -> Option<Self>;
    }

    impl FromWhole<f64> for UnifiedNum {
        fn from_whole(number: f64) -> Self {
            Self::from_whole_opt(number).expect("The number is too large")
        }

        fn from_whole_opt(number: f64) -> Option<Self> {
            let whole_number = number.trunc().to_u64()?.checked_mul(Self::MULTIPLIER)?;

            // multiply the fractional part by the multiplier
            // truncate it to get the fractional part only
            // convert it to u64
            let fract_number = (number.fract() * 10_f64.powf(Self::PRECISION.into()))
                .round()
                .to_u64()?;

            whole_number.checked_add(fract_number).map(Self)
        }
    }

    impl FromWhole<u64> for UnifiedNum {
        fn from_whole(whole_number: u64) -> Self {
            Self(
                whole_number
                    .checked_mul(UnifiedNum::MULTIPLIER)
                    .expect("The whole number is too large"),
            )
        }

        fn from_whole_opt(whole_number: u64) -> Option<Self> {
            whole_number.checked_mul(UnifiedNum::MULTIPLIER).map(Self)
        }
    }

    #[cfg(test)]
    mod test {
        use crate::UnifiedNum;

        use super::FromWhole;

        #[test]
        fn test_whole_number_impl_for_f64() {
            assert_eq!(
                UnifiedNum::from(800_000_000_u64),
                UnifiedNum::from_whole(8.0_f64)
            );
            assert_eq!(
                UnifiedNum::from(810_000_000_u64),
                UnifiedNum::from_whole(8.1_f64)
            );
            assert_eq!(
                UnifiedNum::from(800_000_009_u64),
                UnifiedNum::from_whole(8.000_000_09_f64)
            );

            assert_eq!(
                UnifiedNum::from(800_000_001_u64),
                UnifiedNum::from_whole(8.000_000_009_f64),
                "Should round up the floating number"
            );

            assert_eq!(
                UnifiedNum::from(800_000_000_u64),
                UnifiedNum::from_whole(8.000_000_004_f64),
                "Should round down the floating number"
            );

            assert_eq!(
                UnifiedNum::from(12_345_678_900_000_000_u64),
                UnifiedNum::from_whole(123_456_789.000_000_004_f64),
                "Should round down the floating number"
            );
        }
    }
}

/// Unified Number with a precision of 8 digits after the decimal point.
///
/// The number can be a maximum of `u64::MAX` (the underlying type),
/// or in a `UnifiedNum` value `184_467_440_737.09551615`.
/// The actual number is handled as a unsigned number and only the display shows the decimal point.
///
/// This number is (de)serialized as a Javascript number which is `f64`.
/// As far as the numbers don't exceed `2**63`, the Javascript number should be sufficient without losing precision
///
/// # Examples
///
/// ```
/// use primitives::UnifiedNum;
/// use serde_json::Value;
///
/// let unified_num = UnifiedNum::from(42_999_987_654_321);
///
/// // Printing the unified num will show the value and the decimal point with precision of `UnifiedNum::PRECISION` (i.e. `8`) numbers after the decimal point
/// assert_eq!("42999987654321", &unified_num.to_string());
///
/// assert_eq!("429999.87654321", &unified_num.to_float_string());
///
/// // Printing the Debug of unified num will show the value and the decimal point with precision of `UnifiedNum::PRECISION` (i.e. `8`) numbers after the decimal point
/// assert_eq!("UnifiedNum(429999.87654321)".to_string(), format!("{:?}", &unified_num));
///
/// // JSON Serializing and Deserializing the `UnifiedNum` yields a string without any decimal points
/// assert_eq!(Value::String("42999987654321".to_string()), serde_json::to_value(unified_num).unwrap());
/// ```
#[derive(
    Clone,
    Copy,
    Num,
    ToPrimitive,
    FromPrimitive,
    Zero,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Display,
    FromStr,
    Serialize,
    Deserialize,
    Hash,
)]
#[serde(into = "String", try_from = "String")]
pub struct UnifiedNum(u64);

impl From<UnifiedNum> for String {
    fn from(unified_num: UnifiedNum) -> Self {
        unified_num.to_string()
    }
}

impl TryFrom<String> for UnifiedNum {
    type Error = ParseError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl UnifiedNum {
    /// The precision of the [`UnifiedNum`] is 8 decimal numbers after the comma.
    pub const PRECISION: u8 = 8;
    /// The whole number multiplier when dealing with a [`UnifiedNum`].
    ///
    /// # Examples
    ///
    /// ```
    /// use primitives::UnifiedNum;
    ///
    /// let whole_number = 8_u64; // we want to represent 8.00_000_000
    ///
    /// assert_eq!(UnifiedNum::from_u64(800_000_000), UnifiedNum::from(whole_number * UnifiedNum::MULTIPLIER));
    /// ```
    pub const MULTIPLIER: u64 = 10_u64.pow(Self::PRECISION as u32);
    pub const DEBUG_DELIMITER: char = '.';

    pub const ZERO: UnifiedNum = UnifiedNum(0);
    /// The whole number `1` as a [`UnifiedNum`].
    /// One (`1`) followed by exactly 8 zeroes (`0`).
    ///
    /// `1.00_000_000`
    /// `100_000_000`
    pub const ONE: UnifiedNum = UnifiedNum(100_000_000);

    pub fn div_floor(&self, other: &Self) -> Self {
        let ratio =
            div_unified_num_to_ratio(self, other).expect("Failed to create ratio for div_floor");

        let whole_number = ratio
            .checked_div(&Ratio::from_integer(UnifiedNum::MULTIPLIER))
            .expect("Should divide with Multiplier for div_floor");

        UnifiedNum::from_whole(whole_number.to_integer())
    }

    /// This method creates a [`UnifiedNum`] from an inner [`u64`] value.
    ///
    /// This method does **not** take into account precision of [`UnifiedNum`]!
    pub const fn from_u64(value: u64) -> Self {
        Self(value)
    }

    /// This method returns the inner [`u64`] representation of the [`UnifiedNum`].
    ///
    /// This method does **not** take into account precision of [`UnifiedNum`]!
    pub const fn to_u64(self) -> u64 {
        self.0
    }

    pub fn to_bignum(self) -> BigNum {
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
    pub fn to_precision(self, precision: u8) -> BigNum {
        let inner = BigNum::from(self.0);

        match precision.cmp(&Self::PRECISION) {
            Ordering::Equal => inner,
            Ordering::Less => inner.div_floor(&BigNum::from(10).pow(Self::PRECISION - precision)),
            Ordering::Greater => inner.mul(&BigNum::from(10).pow(precision - Self::PRECISION)),
        }
    }

    /// Transform the BigNum of a given precision to UnifiedNum with precision 8
    /// If the resulting value is larger that what UnifiedNum can hold, it will return `None`
    pub fn from_precision(amount: BigNum, precision: u8) -> Option<Self> {
        // conversation to the UnifiedNum precision is happening with BigNum
        let from_precision = match precision.cmp(&Self::PRECISION) {
            Ordering::Equal => amount,
            Ordering::Less => amount.mul(&BigNum::from(10).pow(Self::PRECISION - precision)),
            Ordering::Greater => {
                amount.div_floor(&BigNum::from(10).pow(precision - Self::PRECISION))
            }
        };
        // only at the end, see if it fits in `u64`
        from_precision.to_u64().map(Self)
    }

    pub fn to_float_string(self) -> String {
        let mut string_value = self.0.to_string();
        let value_length = string_value.len();
        let precision: usize = Self::PRECISION.into();

        if value_length > precision {
            string_value.insert(value_length - precision, Self::DEBUG_DELIMITER);

            string_value
        } else {
            format!("0{}{:0>8}", Self::DEBUG_DELIMITER, string_value)
        }
    }
}

impl From<u64> for UnifiedNum {
    fn from(number: u64) -> Self {
        Self(number)
    }
}

impl fmt::Debug for UnifiedNum {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let float_string = self.to_float_string();

        write!(f, "UnifiedNum({})", float_string)
    }
}

impl One for UnifiedNum {
    /// 1.00_000_000
    fn one() -> Self {
        UnifiedNum::ONE
    }
}

impl Integer for UnifiedNum {
    fn div_floor(&self, other: &Self) -> Self {
        UnifiedNum::div_floor(self, other)
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

impl Add<UnifiedNum> for UnifiedNum {
    type Output = UnifiedNum;

    fn add(self, rhs: UnifiedNum) -> Self::Output {
        UnifiedNum(self.0 + rhs.0)
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

impl Sub<UnifiedNum> for UnifiedNum {
    type Output = UnifiedNum;

    fn sub(self, rhs: UnifiedNum) -> Self::Output {
        UnifiedNum(self.0 - rhs.0)
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
        self.checked_div(rhs).expect("Division by 0")
    }
}

impl Div<UnifiedNum> for UnifiedNum {
    type Output = UnifiedNum;

    fn div(self, rhs: UnifiedNum) -> Self::Output {
        // Use &UnifiedNum / &UnifiedNum
        &self / &rhs
    }
}

impl Div<&UnifiedNum> for UnifiedNum {
    type Output = UnifiedNum;

    #[allow(clippy::op_ref)]
    fn div(self, rhs: &UnifiedNum) -> Self::Output {
        // use &UnifiedNum / &UnifiedNum
        &self / rhs
    }
}

impl Mul<UnifiedNum> for UnifiedNum {
    type Output = UnifiedNum;

    fn mul(self, rhs: UnifiedNum) -> Self::Output {
        &self * &rhs
    }
}

impl Mul<&UnifiedNum> for &UnifiedNum {
    type Output = UnifiedNum;

    fn mul(self, rhs: &UnifiedNum) -> Self::Output {
        // checks for denom = 0 and panics if it is
        // No need for `checked_div`, because MULTIPLIER is always > 0
        let ratio = Ratio::from_integer(self.0) * Ratio::new(rhs.0, UnifiedNum::MULTIPLIER);

        UnifiedNum(ratio.round().to_integer())
    }
}

impl Mul<&UnifiedNum> for UnifiedNum {
    type Output = UnifiedNum;

    #[allow(clippy::op_ref)]
    fn mul(self, rhs: &UnifiedNum) -> Self::Output {
        // Use &UnifiedNum * &UnifiedNum
        &self * rhs
    }
}

impl Rem<UnifiedNum> for UnifiedNum {
    type Output = UnifiedNum;

    fn rem(self, rhs: UnifiedNum) -> Self::Output {
        UnifiedNum(self.0.rem(rhs.0))
    }
}

impl Rem<&UnifiedNum> for &UnifiedNum {
    type Output = UnifiedNum;

    fn rem(self, rhs: &UnifiedNum) -> Self::Output {
        UnifiedNum(self.0.rem(rhs.0))
    }
}

impl CheckedRem for UnifiedNum {
    fn checked_rem(&self, v: &Self) -> Option<Self> {
        self.0.checked_rem(v.0).map(Self)
    }
}

impl<'a> Sum<&'a UnifiedNum> for Option<UnifiedNum> {
    fn sum<I: Iterator<Item = &'a UnifiedNum>>(mut iter: I) -> Self {
        iter.try_fold(0_u64, |acc, unified| acc.checked_add(unified.0))
            .map(UnifiedNum)
    }
}

impl Sum<UnifiedNum> for Option<UnifiedNum> {
    fn sum<I: Iterator<Item = UnifiedNum>>(mut iter: I) -> Self {
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
        let ratio =
            Ratio::from_integer(self.0).checked_mul(&Ratio::new(v.0, UnifiedNum::MULTIPLIER));

        ratio.map(|ratio| Self(ratio.round().to_integer()))
    }
}

impl CheckedDiv for UnifiedNum {
    fn checked_div(&self, rhs: &Self) -> Option<Self> {
        div_unified_num_to_ratio(self, rhs).map(|ratio| UnifiedNum(ratio.floor().to_integer()))
    }
}

/// Flooring, rounding and ceiling of the [`Ratio<u64>`] will produce [`u64`] and **not** a [`UnifiedNum`] ready to use value.
///
/// This means that while `1_u64 / 2_u64 = 0.5` (for [`u64`]) should be rounded to `1_u64`, ceiled to `1_u64` and floored to `0_u64`,
/// the same is not applicable to [`UnifiedNum`].
///
/// [`UnifiedNum`] should be rounded based on the [`UnifiedNum::MULTIPLIER`],
/// i.e. `UnifiedNum(1_u64)` (or `0.00 000 001`) should be rounded to `0` and ceiled to `UnifiedNum::ONE`
/// (`1_00_000_000_u64` or `1.00 000 000` in [`UnifiedNum`] precision)
fn div_unified_num_to_ratio(lhs: &UnifiedNum, rhs: &UnifiedNum) -> Option<Ratio<u64>> {
    if rhs == &UnifiedNum::ONE {
        return Some(Ratio::from_integer(lhs.0));
    }

    // check for denom = 0 because Ration will panic if it is
    if rhs == &UnifiedNum::ZERO {
        return None;
    }

    Ratio::new(lhs.0, rhs.0).checked_mul(&Ratio::new(UnifiedNum::MULTIPLIER, 1))
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
    fn test_unified_num_div_to_u64_ratio() {
        let one = UnifiedNum::one();
        let two = one + one;
        let twenty = UnifiedNum::from_whole(20);
        let three = one + one + one;
        let zero = UnifiedNum::zero();
        let one_tenth = UnifiedNum::from_whole(0.1);
        // 0.00 000 001 = UnifiedNum(1)
        // the smallest representable value of UnifiedNum
        let smallest = UnifiedNum::from(1);
        // 0.00 000 015 = UnifiedNum(15)
        let fifteen = UnifiedNum::from(15);

        // 20 / 2 = 10
        assert_eq!(
            10 * UnifiedNum::MULTIPLIER,
            div_unified_num_to_ratio(&twenty, &two)
                .unwrap()
                .to_integer(),
        );

        // 2 / 0.1 = 20
        assert_eq!(
            20 * UnifiedNum::MULTIPLIER,
            div_unified_num_to_ratio(&two, &one_tenth)
                .unwrap()
                .to_integer()
        );

        // 3 / 0.1 = 30
        assert_eq!(
            30 * UnifiedNum::MULTIPLIER,
            div_unified_num_to_ratio(&three, &one_tenth)
                .unwrap()
                .to_integer()
        );

        // 1 / 0.1 = 10
        assert_eq!(
            10 * UnifiedNum::MULTIPLIER,
            div_unified_num_to_ratio(&one, &one_tenth)
                .unwrap()
                .to_integer()
        );

        // 0.1 / 1 = 0.1
        assert_eq!(
            10_000_000_u64,
            div_unified_num_to_ratio(&one_tenth, &one)
                .unwrap()
                .to_integer()
        );

        // 0.1 / 2 = 0.05
        assert_eq!(
            5_000_000,
            div_unified_num_to_ratio(&one_tenth, &two)
                .unwrap()
                .to_integer()
        );

        // 0.00 000 001
        // 0.00 000 001 / 2.0 = 0.00 000 001
        // should round
        assert_eq!(
            1_u64,
            div_unified_num_to_ratio(&smallest, &two)
                .unwrap()
                .round()
                .to_integer()
        );

        // 0.00 000 000
        // should floor
        assert_eq!(
            0,
            div_unified_num_to_ratio(&smallest, &two)
                .unwrap()
                .floor()
                .to_integer()
        );

        // 0.00 000 015 / 2 = 0.00 000 007 5
        // 0.00 000 008 (when rounding)
        // 0.00 000 007 (when flooring)
        assert_eq!(
            8,
            div_unified_num_to_ratio(&fifteen, &two)
                .unwrap()
                .round()
                .to_integer()
        );

        assert_eq!(
            7,
            div_unified_num_to_ratio(&fifteen, &two)
                .unwrap()
                .floor()
                .to_integer()
        );

        // should ceil to smallest
        // 0.00 000 001 / 3.0 = 0.00 000 000 3333..
        assert_eq!(
            smallest.to_u64(),
            div_unified_num_to_ratio(&smallest, &three)
                .unwrap()
                .ceil()
                .to_integer()
        );
        // Check Division with zero & Zero division
        {
            assert_eq!(None, div_unified_num_to_ratio(&one, &zero), "Division by 0");
            assert_eq!(
                zero,
                UnifiedNum(
                    div_unified_num_to_ratio(&zero, &one)
                        .unwrap()
                        .round()
                        .to_integer()
                ),
                "0 divided by any number is 0"
            );
        }
    }

    #[test]
    fn test_unified_num_displays_debug_and_de_serializes_correctly() {
        let zero = UnifiedNum::zero();
        let one = {
            let manual_one = UnifiedNum::from(100_000_000);
            let impl_one = UnifiedNum::one();
            assert_eq!(manual_one, impl_one);

            manual_one
        };
        let zero_point_one = UnifiedNum::from(10_000_000);
        let smallest_value = UnifiedNum::from(1);
        let random_value = UnifiedNum::from(144_903_000_567_000);

        let dbg_format = |unified_num| -> String { format!("{:?}", unified_num) };

        assert_eq!("UnifiedNum(1.00000000)", &dbg_format(&one));
        assert_eq!("UnifiedNum(0.00000000)", &dbg_format(&zero));
        assert_eq!("UnifiedNum(0.10000000)", &dbg_format(&zero_point_one));
        assert_eq!("UnifiedNum(0.00000001)", &dbg_format(&smallest_value));
        assert_eq!("UnifiedNum(1449030.00567000)", &dbg_format(&random_value));

        let expected_one_string = "100000000".to_string();

        assert_eq!(&expected_one_string, &one.to_string());
        assert_eq!(
            serde_json::Value::String(expected_one_string),
            serde_json::to_value(one).expect("Should serialize")
        )
    }

    #[test]
    fn test_unified_num_convert_to_new_precision_and_from_precision() {
        let dai_precision: u8 = 18;
        let usdt_precision: u8 = 6;
        let same_precision = UnifiedNum::PRECISION;

        let dai_power = BigNum::from(10).pow(BigNum::from(dai_precision as u64));

        // 321.00000000
        let dai_unified = UnifiedNum::from(32_100_000_000_u64);
        let dai_expected = BigNum::from(321_u64) * dai_power;
        let dai_bignum = dai_unified.to_precision(dai_precision);
        assert_eq!(dai_expected, dai_bignum);
        assert_eq!(
            dai_unified,
            UnifiedNum::from_precision(dai_bignum, dai_precision)
                .expect("Should not overflow the UnifiedNum")
        );

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

        // `u64::MAX + 1` should return `None`
        let larger_bignum = BigNum::from(u64::MAX) + BigNum::from(1);

        // USDT - 18446744073709.551616
        assert!(UnifiedNum::from_precision(larger_bignum.clone(), usdt_precision).is_none());

        assert_eq!(
            // DAI - 18.446744073709551616 (MAX + 1)
            Some(UnifiedNum::from(1844674407)),
            // UnifiedNum - 18.44674407
            UnifiedNum::from_precision(larger_bignum, dai_precision),
            "Should floor the large BigNum"
        );
    }

    #[test]
    fn test_unified_num_mul_and_div_and_div_floor() {
        // 0.0003
        let three_ten_thousands = UnifiedNum::from(30_000_u64);
        // 0.1
        let one_tenth = UnifiedNum::from(10_000_000_u64);

        // 1.0
        let one = UnifiedNum::ONE;

        // 2.0
        let two = UnifiedNum::from(200_000_000);

        // 3.0
        let three = UnifiedNum::from(300_000_000);

        let fifteen = UnifiedNum::from_whole(15);

        // division
        {
            // 0.0003 / 0.1 = 0.003
            assert_eq!(UnifiedNum::from(300_000), three_ten_thousands / one_tenth);

            // 3.0 / 0.1 = 30.0
            assert_eq!(UnifiedNum::from_whole(30), three / one_tenth);
            // 2.0 / 0.1 = 20.0
            assert_eq!(UnifiedNum::from_whole(20), two / one_tenth);
            // 1.0 / 0.1 = 10.0
            assert_eq!(UnifiedNum::from_whole(10), one / one_tenth);

            // 3.0 / 1.0 = 3.0
            assert_eq!(three, three / one);

            // 3.0 / 2.0 = 1.5
            assert_eq!(UnifiedNum::from(150_000_000), three / two);

            // 2.0 / 3.0 = 0.6666666...
            assert_eq!(UnifiedNum::from(66_666_666), two / three);

            // 0.1 / 3.0 = 0.03333333
            assert_eq!(UnifiedNum::from(3_333_333), one_tenth / three);

            // 0.1 / 2.0 = 0.05
            assert_eq!(UnifiedNum::from_whole(0.05), one_tenth / two);

            // 0.1 / 1.0 = 0.1
            assert_eq!(one_tenth, one_tenth / one);

            // 15.0 / 2 = 7.5
            assert_eq!(UnifiedNum::from_whole(7.5), fifteen / two);
        }

        // multiplication
        {
            // 0.0003 * 1 = 0.0003
            assert_eq!(three_ten_thousands * one, three_ten_thousands);

            // 3 * 1 = 3
            assert_eq!(three * one, three);

            // 0.0003 * 0.1 = 0.00003
            assert_eq!(three_ten_thousands * one_tenth, UnifiedNum::from(3_000_u64));

            // 0.0003 * 2 = 0.0006
            assert_eq!(three_ten_thousands * two, UnifiedNum::from(60_000_u64));

            // 3 * 2 = 6
            assert_eq!(three * two, UnifiedNum::from(600_000_000_u64));

            // 3 * 0.1 = 0.3
            assert_eq!(three * one_tenth, UnifiedNum::from(30_000_000_u64));
        }

        // Mul & then Div with `checked_mul` & `checked_div`
        {
            // 0.0003 * 0.1 / 1000.0 = 0.00 000 003
            // 30 000 * 10 000 000 / 1 000 00 000 000 = 3
            let result = UnifiedNum::from_whole(0.0003)
                .checked_mul(&UnifiedNum::from_whole(0.1))
                .and_then(|number| number.checked_div(&UnifiedNum::from_whole(1_000)))
                .unwrap();

            assert_eq!(UnifiedNum::from(3), result);

            let result = UnifiedNum::from_whole(0.0003)
                .checked_mul(&UnifiedNum::from_whole(0.1))
                .and_then(|number| number.checked_div(&UnifiedNum::from_whole(1000)))
                .unwrap();

            assert_eq!(UnifiedNum::from(3), result);
        }

        // div_floor
        {
            // 1.2 / 2 = 0.6 = 0.0 (floored)
            let result = UnifiedNum::from_whole(1.2).div_floor(&UnifiedNum::from_whole(2));
            assert_eq!(UnifiedNum::ZERO, result);

            // 3.8 / 2 = 1.9 = 1.0 (floored)
            let result = UnifiedNum::from_whole(3.2).div_floor(&UnifiedNum::from_whole(2));
            assert_eq!(UnifiedNum::ONE, result);

            // 15 / 2 = 7 (floored)
            assert_eq!(UnifiedNum::from_whole(7), fifteen.div_floor(&two));
        }
    }

    #[test]
    fn test_unified_num_rem_and_checked_rem_and_with_whole() {
        // 10.0 % 3.0 = 1.0
        {
            assert_eq!(
                UnifiedNum::ONE,
                UnifiedNum::from(1_000_000_000) % UnifiedNum::from(300_000_000)
            );

            assert_eq!(
                UnifiedNum::ONE,
                UnifiedNum::from_whole(10) % UnifiedNum::from_whole(3)
            );

            assert_eq!(
                UnifiedNum::ONE,
                UnifiedNum::from_whole(10.0) % UnifiedNum::from_whole(3.0)
            );
            assert_eq!(
                UnifiedNum::from(100_000_000),
                UnifiedNum::from_whole(10) % UnifiedNum::from_whole(3)
            );
        }

        // 10.0 % 0.3 = 0.1
        {
            assert_eq!(
                UnifiedNum::from_whole(10.0),
                UnifiedNum::from(1_000_000_000) % UnifiedNum::from_whole(30_000_000)
            );

            assert_eq!(
                UnifiedNum::from(10_000_000),
                UnifiedNum::from_whole(10.0) % UnifiedNum::from_whole(0.3)
            );

            assert_eq!(
                UnifiedNum::from(10_000_000),
                UnifiedNum::from_whole(10) % UnifiedNum::from_whole(0.3)
            );
        }

        // 10.0 % 0.03 = 0.01
        {
            assert_eq!(
                UnifiedNum::from_whole(10.0),
                UnifiedNum::from(1_000_000_000) % UnifiedNum::from_whole(3_000_000)
            );

            assert_eq!(
                UnifiedNum::from(1_000_000),
                UnifiedNum::from_whole(10.0) % UnifiedNum::from_whole(0.03)
            );

            assert_eq!(
                UnifiedNum::from(1_000_000),
                UnifiedNum::from_whole(10) % UnifiedNum::from_whole(0.03)
            );
        }

        // 0.3 % 10.0 = 0.3
        {
            assert_eq!(
                UnifiedNum::from(30_000_000),
                UnifiedNum::from(30_000_000) % UnifiedNum::from(1_000_000_000)
            );

            assert_eq!(
                UnifiedNum::from_whole(0.3),
                UnifiedNum::from_whole(0.3) % UnifiedNum::from_whole(10.0)
            );
        }

        // CheckedRem by 0
        {
            assert_eq!(
                None,
                UnifiedNum::from_whole(3).checked_rem(&UnifiedNum::ZERO),
                "CheckedRem by zero should result in None"
            );
        }
    }

    #[test]
    fn test_unified_num_mod_floor_gcd_lcm_divides_is_multiple_of_div_rem() {
        // Mod floor
        {
            assert_eq!(
                (UnifiedNum::from_u64(8)).mod_floor(&UnifiedNum::from_u64(3)),
                UnifiedNum::from_u64(2)
            );
            assert_eq!(
                (UnifiedNum::from_u64(1)).mod_floor(&UnifiedNum::from_u64(2)),
                UnifiedNum::from_u64(1)
            );
        }

        // GCD
        {
            assert_eq!(
                UnifiedNum::from_u64(6).gcd(&UnifiedNum::from_u64(8)),
                UnifiedNum::from_u64(2)
            );
            assert_eq!(
                UnifiedNum::from_u64(7).gcd(&UnifiedNum::from_u64(3)),
                UnifiedNum::from_u64(1)
            );
        }

        // LCM
        {
            assert_eq!(
                UnifiedNum::from_u64(7).lcm(&UnifiedNum::from_u64(3)),
                UnifiedNum::from_u64(21)
            );
            assert_eq!(
                UnifiedNum::from_u64(2).lcm(&UnifiedNum::from_u64(4)),
                UnifiedNum::from_u64(4)
            );
        }

        // Is multiple of
        {
            assert_eq!(
                UnifiedNum::from_u64(9).is_multiple_of(&UnifiedNum::from_u64(3)),
                true
            );
            assert_eq!(
                UnifiedNum::from_u64(3).is_multiple_of(&UnifiedNum::from_u64(9)),
                false
            );
        }

        // Div rem
        {
            assert_eq!(
                (UnifiedNum::from_u64(8)).div_rem(&UnifiedNum::from_u64(3)),
                (UnifiedNum::from_u64(2), UnifiedNum::from_u64(2))
            );
            assert_eq!(
                (UnifiedNum::from_u64(1)).div_rem(&UnifiedNum::from_u64(2)),
                (UnifiedNum::from_u64(0), UnifiedNum::from_u64(1))
            );
        }
    }
}

#[cfg(feature = "postgres")]
mod postgres {
    use super::UnifiedNum;
    use bytes::BytesMut;
    use std::error::Error;
    use tokio_postgres::types::{accepts, to_sql_checked, FromSql, IsNull, ToSql, Type};

    impl<'a> FromSql<'a> for UnifiedNum {
        fn from_sql(ty: &Type, raw: &'a [u8]) -> Result<UnifiedNum, Box<dyn Error + Sync + Send>> {
            let value = <i64 as FromSql>::from_sql(ty, raw)?;

            Ok(UnifiedNum(u64::try_from(value)?))
        }

        accepts!(INT8);
    }

    impl ToSql for UnifiedNum {
        fn to_sql(
            &self,
            ty: &Type,
            w: &mut BytesMut,
        ) -> Result<IsNull, Box<dyn Error + Sync + Send>> {
            <i64 as ToSql>::to_sql(&self.0.try_into()?, ty, w)
        }

        accepts!(INT8);

        to_sql_checked!();
    }

    #[cfg(test)]
    mod test {
        use super::*;
        use crate::postgres::POSTGRES_POOL;

        #[tokio::test]
        async fn from_and_to_sql() {
            let client = POSTGRES_POOL.get().await.unwrap();

            let sql_type = "BIGINT";
            let (val, repr) = (
                UnifiedNum(9_223_372_036_854_775_708_u64),
                "9223372036854775708",
            );

            // from SQL
            {
                let rows = client
                    .query(&*format!("SELECT {}::{}", repr, sql_type), &[])
                    .await
                    .unwrap();
                let result: UnifiedNum = rows[0].get(0);

                assert_eq!(&val, &result);
            }

            // to SQL
            {
                let rows = client
                    .query(&*format!("SELECT $1::{}", sql_type), &[&val])
                    .await
                    .unwrap();
                let result = rows[0].get(0);
                assert_eq!(&val, &result);
            }
        }
    }
}
