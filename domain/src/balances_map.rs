use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{BigNum, Channel, DomainError, ValidatorDesc};

type InnerBTreeMap = BTreeMap<String, BigNum>;

#[derive(Default, Clone, Debug, Serialize, Deserialize)]
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
        fee_ratio: BigNum,
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

struct Distribution {
    pub deposit: BigNum,
    /// Total Distributed is the sum of all balances in the BalancesMap
    pub total_distributed: BigNum,
    /// The Sum of all validators fee
    pub validators_fee: BigNum,
    /// Deposit - Validators fee
    pub to_distribute: BigNum,
    /// The ratio that is (Deposit - TotalValidatorFee) / Deposit
    pub ratio: BigNum,
    /// Total Distributed / Deposit
    pub fee_ratio: BigNum,
    _secret: (),
}

impl Distribution {
    pub fn new(for_balances: &InnerBTreeMap, on_channel: &Channel) -> Result<Self, DomainError> {
        let deposit = on_channel.deposit_amount.clone();

        let total_distributed: BigNum = for_balances.iter().map(|(_, balance)| balance).sum();

        let validators_iter = on_channel.spec.validators.into_iter();
        let total_validators_fee: BigNum = validators_iter.map(|validator| &validator.fee).sum();

        if total_validators_fee <= deposit {
            return Err(DomainError::RuleViolation(
                "total fees <= deposit: fee constraint violated".into(),
            ));
        }

        if total_distributed <= deposit {
            return Err(DomainError::RuleViolation(
                "distributed <= deposit: OUTPACE rule #4".into(),
            ));
        }

        let to_distribute = &deposit - &total_validators_fee;
        let ratio = &to_distribute / &deposit;
        let fee_ratio = &total_distributed / &deposit;

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
