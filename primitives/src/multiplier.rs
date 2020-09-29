use crate::BigNum;
use std::{cmp::Ordering, num::NonZeroU32, num::NonZeroU8, ops::Mul};

lazy_static::lazy_static! {
    /// DAI has precision of 18 decimals
    /// For CPM we have 3 decimals precision, but that's for 1000 (3 decimals more)
    /// This in terms means we need 18 - (3 + 3) = 12 decimals precision
    pub static ref GLOBAL_MULTIPLIER: Multiplier = M12.clone();
    pub static ref M12: Multiplier = Multiplier::new(NonZeroU8::new(12).expect("OK"));
    pub static ref M18: Multiplier = Multiplier::new(NonZeroU8::new(18).expect("OK"));
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Multiplier(BigNum);

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct MultiplierNum(BigNum, Multiplier);

impl MultiplierNum {
    pub fn new(value: BigNum, multiplier: Multiplier) -> Self {
        Self(value, multiplier)
    }

    pub fn to_multiplier(&self, to_multiplier: Multiplier) -> Self {
        convert_multipliers(to_multiplier, self.clone())
    }
}

impl Multiplier {
    fn new(multiplier: NonZeroU8) -> Self {
        let multiplier_pow = u32::from(NonZeroU32::from(multiplier));

        Self(BigNum::from(10_u64).pow(multiplier_pow))
    }
}

impl std::ops::Sub<&Multiplier> for &Multiplier {
    type Output = (Ordering, Multiplier);

    fn sub(self, rhs: &Multiplier) -> Self::Output {
        let order = self.0.cmp(&rhs.0);

        match &order {
            Ordering::Less => (order, Multiplier((&rhs.0).div_floor(&self.0))),
            Ordering::Equal => (order, Multiplier(self.0.to_owned())),
            Ordering::Greater => (order, Multiplier((&self.0).div_floor(&rhs.0))),
        }
    }
}

impl std::ops::Sub<Multiplier> for MultiplierNum {
    type Output = MultiplierNum;

    fn sub(self, rhs: Multiplier) -> Self::Output {
        convert_multipliers(rhs, self)
    }
}

pub fn convert_multipliers(into_multiplier: Multiplier, from_num: MultiplierNum) -> MultiplierNum {
    match &into_multiplier - &from_num.1 {
        (Ordering::Less, multiplier) => {
            MultiplierNum(from_num.0.div_floor(&multiplier.0), into_multiplier)
        }
        (Ordering::Greater, multiplier) => {
            MultiplierNum(from_num.0.mul(&multiplier.0), into_multiplier)
        }
        (Ordering::Equal, _) => from_num,
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_multiplier_and_multiplier_subtraction() {
        let ten = Multiplier::new(NonZeroU8::new(10).unwrap());

        assert_eq!(&BigNum::from(10_000_000_000_u64), &ten.0);

        let twenty = Multiplier::new(NonZeroU8::new(20).unwrap());

        assert_eq!((Ordering::Greater, ten.clone()), &twenty - &ten);
        assert_eq!((Ordering::Less, ten.clone()), &ten - &twenty);
        assert_eq!((Ordering::Equal, ten.clone()), &ten - &ten);
    }

    #[test]
    fn test_convert_multipliers_roundtrip() {
        let dai_multiplier = Multiplier::new(NonZeroU8::new(18).unwrap());
        let other_multiplier = Multiplier::new(NonZeroU8::new(30).unwrap());

        let input_value = MultiplierNum(
            BigNum::from(321_000_000_000_000_u64),
            other_multiplier.clone(),
        );

        let dai_actual = convert_multipliers(dai_multiplier.clone(), input_value.clone());
        let dai_expected = MultiplierNum(BigNum::from(321_u64), dai_multiplier);
        assert_eq!(dai_actual, dai_expected);

        let other_actual = convert_multipliers(other_multiplier, dai_actual);

        assert_eq!(
            other_actual, input_value,
            "No flooring involved so it should result in the same as input"
        );
    }

    #[test]
    fn test_convert_multipliers_roundtrip_with_flooring() {
        let dai_multiplier = Multiplier::new(NonZeroU8::new(18).unwrap());
        let other_multiplier = Multiplier::new(NonZeroU8::new(30).unwrap());

        let input_value = MultiplierNum(
            BigNum::from(321_999_999_999_999_u64),
            other_multiplier.clone(),
        );

        let dai_actual = convert_multipliers(dai_multiplier.clone(), input_value.clone());
        let dai_expected = MultiplierNum(BigNum::from(321_u64), dai_multiplier);

        assert_eq!(dai_actual, dai_expected);

        let other_actual = convert_multipliers(other_multiplier.clone(), dai_actual);
        let other_expected = MultiplierNum(BigNum::from(321_000_000_000_000_u64), other_multiplier);

        assert_eq!(other_actual, other_expected);
    }
}
