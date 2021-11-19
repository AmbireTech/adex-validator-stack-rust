pub mod fee {
    pub const PRO_MILLE: UnifiedNum = UnifiedNum::from_u64(1_000);

    use primitives::{Address, DomainError, UnifiedNum, ValidatorDesc};

    /// Calculates the fee for a given payout of the specified validator
    /// This function will return None if the provided validator is not part of the Campaign / Channel
    /// In the case of overflow when calculating the payout, an error will be returned
    pub fn calculate_fee(
        (_earner, payout): (Address, UnifiedNum),
        validator: &ValidatorDesc,
    ) -> Result<UnifiedNum, DomainError> {
        // should never overflow
        payout
            .checked_mul(&validator.fee)
            .map(|pro_mille_fee| pro_mille_fee.div_floor(&PRO_MILLE))
            .ok_or_else(|| DomainError::InvalidArgument("payout calculation overflow".to_string()))
    }

    #[cfg(test)]
    mod test {
        use primitives::{
            test_util::{PUBLISHER, DUMMY_VALIDATOR_LEADER}, UnifiedNum,
        };

        use crate::spender::fee::calculate_fee;

        #[test]
        fn test_calcualtion_of_fee() {
            let dummy_leader = DUMMY_VALIDATOR_LEADER.clone();
            assert_eq!(
                UnifiedNum::from(100),
                dummy_leader.fee,
                "Dummy validator leader fee has changed, please revisit this test!"
            );

            // normal payout - no flooring
            {
                // 300 * 100 / 1000 = 30
                let payout = (*PUBLISHER, UnifiedNum::from(300));

                let validator_fee =
                    calculate_fee(payout, &dummy_leader).expect("Should not overflow");

                assert_eq!(UnifiedNum::from(30), validator_fee);
            }

            // payout with flooring
            {
                // 66 * 100 / 1000 = 6.6 = 6
                let payout = (*PUBLISHER, UnifiedNum::from(66));
                let validator_fee =
                    calculate_fee(payout, &dummy_leader).expect("Should not overflow");

                assert_eq!(UnifiedNum::from(6), validator_fee);
            }

            // Overflow
            {
                // u64::MAX * 100 (overflow) / 1000
                let payout = (*PUBLISHER, UnifiedNum::from(u64::MAX));

                calculate_fee(payout, &dummy_leader).expect_err("Should overflow");
            }
        }
    }
}
