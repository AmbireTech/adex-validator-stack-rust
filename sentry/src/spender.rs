pub mod fee {

    use primitives::{Address, DomainError, UnifiedNum, ValidatorDesc};

    /// Pro mile (`1 000`) in [`UnifiedNum`] precision, i.e. `1_000.00_000_000`.
    pub const PRO_MILLE: UnifiedNum = UnifiedNum::from_u64(1_000 * UnifiedNum::MULTIPLIER);

    /// Calculates the fee for a given payout of the specified validator
    /// This function will return None if the provided validator is not part of the Campaign / Channel
    /// In the case of overflow when calculating the payout, an error will be returned
    pub fn calculate_fee(
        (_earner, payout): (Address, UnifiedNum),
        validator: &ValidatorDesc,
    ) -> Result<UnifiedNum, DomainError> {
        // should never overflow, but we guard against overflow
        // `UnifiedNum::checked_div` will accurately floor the inner `u64` for the `UnifiedNum::PRECISION`
        payout
            .checked_mul(&validator.fee)
            .and_then(|pro_mille_fee| pro_mille_fee.checked_div(&PRO_MILLE))
            .ok_or_else(|| DomainError::InvalidArgument("payout calculation overflow".to_string()))
    }

    #[cfg(test)]
    mod test {
        use primitives::{
            test_util::{DUMMY_VALIDATOR_LEADER, PUBLISHER},
            UnifiedNum, ValidatorDesc,
        };

        use crate::spender::fee::calculate_fee;

        #[test]
        fn test_calculation_of_fee() {
            let mut dummy_leader = DUMMY_VALIDATOR_LEADER.clone();
            dummy_leader.fee = UnifiedNum::from(10_000_000);

            // normal payout - no flooring
            {
                // 30 000 * 10 000 000 / 1 000 00 000 000 = 3

                // 0.0003 * 0.1 / 1000.0 = 0.00000003 = UnifiedNum(3)
                // 0.00 030 000 * 0.10 000 000 / 1 000 = 0.00 000 003
                let payout = (*PUBLISHER, UnifiedNum::from(30_000));

                let validator_fee =
                    calculate_fee(payout, &dummy_leader).expect("Should not overflow");

                assert_eq!(
                    UnifiedNum::from(3),
                    validator_fee,
                    "fee should be 0.00000003 in UnifiedNum"
                );
            }

            // payout with flooring
            {
                // 66 000 * 10 000 000 / 100 000 000 000 = 6.6 = 6
                let payout = (*PUBLISHER, UnifiedNum::from(66_000));
                let validator_fee =
                    calculate_fee(payout, &dummy_leader).expect("Should not overflow");

                assert_eq!(UnifiedNum::from(6), validator_fee);
            }

            // Overflow - even using `Ratio` for `UnifiedNum`, this should overflow
            {
                let very_high_fee = ValidatorDesc {
                    fee: UnifiedNum::from(u64::MAX),
                    ..dummy_leader.clone()
                };
                // u64::MAX * u64::MAX / 100 000 000 000
                let payout = (*PUBLISHER, UnifiedNum::from(u64::MAX));

                calculate_fee(payout, &very_high_fee).expect_err("Should overflow");
            }

            // whole number payout
            {
                // e.g. 3 TOKENs
                let payout = (*PUBLISHER, UnifiedNum::from(300_000_000_u64));

                // 300 000 000 × 10 000 000 / 100 000 000 000
                let validator_fee =
                    calculate_fee(payout, &dummy_leader).expect("Should not overflow");

                // : 3 000 000 000 000
                // 0.000003
                assert_eq!(UnifiedNum::from(30_000), validator_fee);
            }
        }
    }
}
