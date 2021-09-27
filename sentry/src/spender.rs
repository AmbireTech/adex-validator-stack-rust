pub mod fee {
    pub const PRO_MILLE: UnifiedNum = UnifiedNum::from_u64(1_000);

    use primitives::{Address, DomainError, UnifiedNum, ValidatorDesc};

    /// Calculates the fee for a specified validator
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
}
