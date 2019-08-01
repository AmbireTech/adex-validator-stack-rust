use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{BigNum, Channel, DomainError, ValidatorDesc};
use num::{rational::Ratio, BigUint};

type InnerBTreeMap = BTreeMap<String, BigNum>;

#[derive(Default, Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(transparent)]
pub struct BalancesMap(InnerBTreeMap);

impl BalancesMap {
    pub fn apply_fees(&self, on_channel: &Channel) -> Result<Self, DomainError> {
        let distribution = Distribution::new(&self.0, &on_channel)?;

        let mut balances_after_fees = BTreeMap::default();
        let mut total = BigNum::from(0);

        for (key, value) in self.0.iter() {
            let adjusted_balance = value * &distribution.ratio;

            total += &adjusted_balance;
            balances_after_fees.insert(key.clone(), adjusted_balance);
        }

        let rounding_error = distribution.rounding_error(&total)?;

        let balances_after_fees = Self::distribute_fee(
            balances_after_fees,
            rounding_error,
            distribution.fee_ratio,
            on_channel.spec.validators.into_iter(),
        );

        Ok(Self(balances_after_fees))
    }

    fn distribute_fee<'a>(
        mut balances: InnerBTreeMap,
        rounding_error: BigNum,
        fee_ratio: Ratio<BigUint>,
        validators: impl Iterator<Item = &'a ValidatorDesc>,
    ) -> InnerBTreeMap {
        for (index, validator) in validators.enumerate() {
            let fee = &validator.fee * &fee_ratio;

            let fee_rounded = if index == 0 {
                &fee + &rounding_error
            } else {
                fee
            };

            if fee_rounded > 0.into() {
                let entry = balances
                    .entry(validator.id.clone())
                    .or_insert_with(|| 0.into());

                *entry += &fee_rounded;
            }
        }

        balances
    }
}

#[derive(Debug)]
struct Distribution {
    pub deposit: BigNum,
    /// Total Distributed is the sum of all balances in the BalancesMap
    pub total_distributed: BigNum,
    /// The Sum of all validators fee
    pub validators_fee: BigNum,
    /// Deposit - Validators fee
    pub to_distribute: BigNum,
    /// The ratio that is (Deposit - TotalValidatorFee) / Deposit
    pub ratio: Ratio<BigUint>,
    /// Total Distributed / Deposit
    pub fee_ratio: Ratio<BigUint>,
    _secret: (),
}

impl Distribution {
    pub fn new(for_balances: &InnerBTreeMap, on_channel: &Channel) -> Result<Self, DomainError> {
        let deposit = on_channel.deposit_amount.clone();

        let total_distributed: BigNum = for_balances.iter().map(|(_, balance)| balance).sum();

        let validators_iter = on_channel.spec.validators.into_iter();
        let total_validators_fee: BigNum = validators_iter.map(|validator| &validator.fee).sum();

        if total_validators_fee > deposit {
            return Err(DomainError::RuleViolation(
                "total fees <= deposit: fee constraint violated".into(),
            ));
        }

        if total_distributed > deposit {
            return Err(DomainError::RuleViolation(
                "distributed <= deposit: OUTPACE rule #4".into(),
            ));
        }

        let to_distribute = &deposit - &total_validators_fee;

        let ratio = to_distribute.ratio(&deposit);
        let fee_ratio = total_distributed.ratio(&deposit);

        Ok(Self {
            deposit,
            total_distributed,
            validators_fee: total_validators_fee,
            to_distribute,
            ratio,
            fee_ratio,
            _secret: (),
        })
    }

    /// Returns the rounding error and also checks for rule violation if it is < 0
    pub fn rounding_error(&self, total_distributed: &BigNum) -> Result<BigNum, DomainError> {
        let rounding_error = if self.deposit == self.total_distributed {
            &self.to_distribute - total_distributed
        } else {
            BigNum::from(0)
        };

        if rounding_error < BigNum::from(0) {
            Err(DomainError::RuleViolation(
                "The Rounding error should never be negative".into(),
            ))
        } else {
            Ok(rounding_error)
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::channel::fixtures::{get_channel, get_channel_spec, ValidatorsOption};
    use crate::validator::fixtures::get_validator;

    mod applying_fee_returns_the_same_tree_with_zero_fees {
        use super::*;
        fn setup_balances_map(tree: &InnerBTreeMap) -> BalancesMap {
            let channel = get_zero_fee_channel();

            let balances_map = BalancesMap(tree.clone());

            let balances_after_fee = balances_map
                .apply_fees(&channel)
                .expect("Calculation of fees failed");

            balances_after_fee
        }

        #[test]
        fn case_1_three_values() {
            let tree: InnerBTreeMap = vec![
                ("a".to_string(), 1001.into()),
                ("b".to_string(), 3124.into()),
                ("c".to_string(), 122.into()),
            ]
            .into_iter()
            .collect();

            assert_eq!(setup_balances_map(&tree).0, tree);
        }

        #[test]
        fn case_2_three_simple_values() {
            let tree: InnerBTreeMap = vec![
                ("a".to_string(), 1.into()),
                ("b".to_string(), 2.into()),
                ("c".to_string(), 3.into()),
            ]
            .into_iter()
            .collect();

            assert_eq!(setup_balances_map(&tree).0, tree);
        }

        #[test]
        fn case_3_one_value() {
            let tree: InnerBTreeMap = vec![("a".to_string(), BigNum::from(1))]
                .into_iter()
                .collect();

            assert_eq!(setup_balances_map(&tree).0, tree);
        }

        #[test]
        fn case_4_two_values() {
            let tree: InnerBTreeMap = vec![
                ("a".to_string(), 1.into()),
                ("b".to_string(), 99_999.into()),
            ]
            .into_iter()
            .collect();

            assert_eq!(setup_balances_map(&tree).0, tree);
        }

        fn get_zero_fee_channel() -> Channel {
            let leader = get_validator("one", Some(0.into()));
            let follower = get_validator("two", Some(0.into()));

            let spec = get_channel_spec(ValidatorsOption::Pair { leader, follower });
            let mut channel = get_channel("zero fees", &None, Some(spec));
            channel.deposit_amount = 100_000.into();

            channel
        }
    }

    mod applying_fee_correctly {
        use super::*;

        fn setup_balances_after_fee(tree: InnerBTreeMap) -> BalancesMap {
            let leader = get_validator("one", Some(50.into()));
            let follower = get_validator("two", Some(50.into()));

            let spec = get_channel_spec(ValidatorsOption::Pair { leader, follower });
            let mut channel = get_channel("apply fees", &None, Some(spec));
            channel.deposit_amount = 10_000.into();

            let balances_map = BalancesMap(tree);

            let balances_after_fee = balances_map
                .apply_fees(&channel)
                .expect("Calculation of fees failed");

            balances_after_fee
        }

        #[test]
        fn case_1_partially_distributed() {
            let tree = vec![
                ("a".to_string(), 1000.into()),
                ("b".to_string(), 1200.into()),
            ]
            .into_iter()
            .collect();

            let expected_tree: InnerBTreeMap = vec![
                ("a".to_string(), 990.into()),
                ("b".to_string(), 1188.into()),
                ("one".to_string(), 11.into()),
                ("two".into(), 11.into()),
            ]
            .into_iter()
            .collect();

            let balances_after_fee = setup_balances_after_fee(tree).0;
            let actual_sum: BigNum = balances_after_fee.iter().map(|(_, v)| v).sum();

            assert_eq!(
                expected_tree.iter().map(|(_, value)| value).sum::<BigNum>(),
                actual_sum
            );
        }
    }

    #[test]
    fn errors_when_fees_larger_that_deposit() {
        let tree: InnerBTreeMap = vec![("a".to_string(), 10.into()), ("b".to_string(), 10.into())]
            .into_iter()
            .collect();

        let leader = get_validator("one", Some(600.into()));
        let follower = get_validator("two", Some(600.into()));
        let spec = get_channel_spec(ValidatorsOption::Pair { leader, follower });
        let mut channel = get_channel("zero fees", &None, Some(spec));
        channel.deposit_amount = 1_000.into();

        let balances_map = BalancesMap(tree.clone());

        let domain_error = balances_map
            .apply_fees(&channel)
            .expect_err("Should be DomainError not allow fees sum to exceed the deposit");

        assert_eq!(
            DomainError::RuleViolation(
                "total fees <= deposit: fee constraint violated".to_string()
            ),
            domain_error
        )
    }

}
