use std::time::Duration;

use dashmap::DashMap;
use primitives::{spender::Aggregate, ChannelId};

#[derive(Debug)]
///
pub struct Aggregator {
    /// In-memory aggregates waiting to be saved to the underlying persistence storage (database)
    aggregates: DashMap<ChannelId, Aggregate>,
    /// Specifies how often the Aggregate should be stored in the underlying persistence storage (database)
    throttle: Duration,
}

impl Aggregator {
    /// Stores the aggregate to the database
    pub fn store_aggregates() {
        todo!("Store aggregate to DB")
    }
    /// Records new spending triggered by a Payout event
    pub async fn record() {
        todo!("Record a new payout")
    }
}

pub mod fee {
    pub const PRO_MILLE: UnifiedNum = UnifiedNum::from_u64(1_000);

    use primitives::{Address, Campaign, DomainError, UnifiedNum, ValidatorId};

    /// Calculates the fee for a specified validator
    /// This function will return None if the provided validator is not part of the Campaign / Channel
    /// In the case of overflow when calculating the payout, an error will be returned
    pub fn calculate_fees(
        (_earner, payout): (Address, UnifiedNum),
        campaign: &Campaign,
        for_validator: ValidatorId,
    ) -> Result<Option<UnifiedNum>, DomainError> {
        let payout = match campaign.find_validator(&for_validator) {
            Some(validator_role) => {
                // should never overflow
                let fee_payout = payout
                    .checked_mul(&validator_role.validator().fee)
                    .ok_or_else(|| {
                        DomainError::InvalidArgument("payout calculation overflow".to_string())
                    })?
                    .div_floor(&PRO_MILLE);

                Some(fee_payout)
            }
            None => None,
        };

        Ok(payout)
    }
}
