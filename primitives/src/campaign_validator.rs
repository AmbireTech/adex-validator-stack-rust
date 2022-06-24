use crate::{
    campaign::Validators, config::Config, Address, Campaign, ChainOf, UnifiedNum, ValidatorId,
};
use chrono::Utc;
use std::cmp::PartialEq;
use thiserror::Error;

pub trait Validator {
    fn validate(
        self,
        config: &Config,
        validator_identity: ValidatorId,
    ) -> Result<ChainOf<Campaign>, Error>;
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Validation {
    /// When the Adapter address is not listed in the `campaign.validators` & `campaign.channel.(leader/follower)`
    /// which in terms means, that the adapter shouldn't handle this Campaign
    AdapterNotIncluded,
    /// when `channel.active.to` has passed (i.e. < now), the Campaign should not be handled
    // campaign.active.to must be in the future
    InvalidActiveTo,
    UnlistedValidator,
    UnlistedCreator,
    UnlistedAsset,
    MinimumValidatorFeeNotMet,
    FeeConstraintViolated,
}

#[derive(Debug, Eq, PartialEq, Clone, Copy, Error)]
pub enum Error {
    #[error("Summing the Validators fee results in overflow")]
    FeeSumOverflow,
    #[error("Validation error: {0:?}")]
    Validation(Validation),
}

impl From<Validation> for Error {
    fn from(v: Validation) -> Self {
        Self::Validation(v)
    }
}

impl Validator for Campaign {
    fn validate(
        self,
        config: &Config,
        validator_identity: ValidatorId,
    ) -> Result<ChainOf<Campaign>, Error> {
        // check if the channel validators include our adapter identity
        let whoami_validator = match self.find_validator(&validator_identity) {
            Some(role) => role.into_inner(),
            None => return Err(Validation::AdapterNotIncluded.into()),
        };

        if self.active.to < Utc::now() {
            return Err(Validation::InvalidActiveTo.into());
        }

        if !all_validators_listed(&self.validators, &config.validators_whitelist) {
            return Err(Validation::UnlistedValidator.into());
        }

        if !creator_listed(&self, &config.creators_whitelist) {
            return Err(Validation::UnlistedCreator.into());
        }

        // Check if Channel token is listed in the configuration token Chain ID & Address
        let chain_context = config
            .find_chain_of(self.channel.token)
            .ok_or(Validation::UnlistedAsset)?;

        // Check if the validator fee is greater than the minimum configured fee
        if whoami_validator
            .fee
            .to_precision(chain_context.token.precision.get())
            < chain_context.token.min_validator_fee
        {
            return Err(Validation::MinimumValidatorFeeNotMet.into());
        }

        let total_validator_fee: UnifiedNum = self
            .validators
            .iter()
            .map(|v| &v.fee)
            .sum::<Option<_>>()
            // on overflow return an error
            .ok_or(Error::FeeSumOverflow)?;

        if total_validator_fee >= self.budget {
            return Err(Validation::FeeConstraintViolated.into());
        }

        Ok(chain_context.with_campaign(self))
    }
}

pub fn all_validators_listed(validators: &Validators, whitelist: &[ValidatorId]) -> bool {
    if whitelist.is_empty() {
        true
    } else {
        let found_validators = whitelist
            .iter()
            .filter(|&allowed| validators.find(allowed).is_some())
            // this will ensure that if we find the 2 validators earlier
            // we don't go over the other values of the whitelist
            .take(2);
        // the found validators should be exactly 2, if they are not, then 1 or 2 are missing
        found_validators.count() == 2
    }
}

pub fn creator_listed(campaign: &Campaign, whitelist: &[Address]) -> bool {
    // if the list is empty, return true, as we don't have a whitelist to restrict us to
    // or if we have a list, check if it includes the `channel.creator`
    whitelist.is_empty()
        || whitelist
            .iter()
            .any(|allowed| allowed.eq(&campaign.creator))
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        config::{self, GANACHE_CONFIG},
        test_util::{
            ADVERTISER, DUMMY_CAMPAIGN, DUMMY_VALIDATOR_FOLLOWER, DUMMY_VALIDATOR_LEADER, FOLLOWER,
            GUARDIAN, IDS, LEADER, PUBLISHER,
        },
        BigNum,
    };
    use chrono::{TimeZone, Utc};
    use std::str::FromStr;

    #[test]
    fn are_validators_listed() {
        let validators = Validators::new((
            DUMMY_VALIDATOR_LEADER.clone(),
            DUMMY_VALIDATOR_FOLLOWER.clone(),
        ));

        // empty whitelist
        let are_listed = all_validators_listed(&validators, &[]);
        assert!(are_listed);
        // no validators listed
        let are_listed = all_validators_listed(&validators, &[IDS[&ADVERTISER], IDS[&GUARDIAN]]);
        assert!(!are_listed);
        // one validator listed
        let are_listed = all_validators_listed(
            &validators,
            &[IDS[&ADVERTISER], IDS[&GUARDIAN], IDS[&LEADER]],
        );
        assert!(!are_listed);
        // both validators lister
        let are_listed = all_validators_listed(
            &validators,
            &[
                IDS[&ADVERTISER],
                IDS[&GUARDIAN],
                IDS[&LEADER],
                IDS[&FOLLOWER],
            ],
        );
        assert!(are_listed);
    }

    #[test]
    fn is_creator_listed() {
        let campaign = DUMMY_CAMPAIGN.clone();

        // empty whitelist
        let is_listed = creator_listed(&campaign, &[]);
        assert!(is_listed);

        // not listed
        let is_listed = creator_listed(&campaign, &[*PUBLISHER]);
        assert!(!is_listed);

        // listed
        let is_listed = creator_listed(&campaign, &[*PUBLISHER, campaign.creator]);
        assert!(is_listed);
    }

    #[test]
    fn chain_and_token_whitelist_validation() {
        let campaign = DUMMY_CAMPAIGN.clone();

        // no configured Chains & Tokens
        {
            let mut config = GANACHE_CONFIG.clone();
            config.chains.clear();

            let result = campaign.clone().validate(&config, campaign.channel.leader);

            assert!(matches!(
                result,
                Err(Error::Validation(Validation::UnlistedAsset))
            ));
        }

        {
            let config = GANACHE_CONFIG.clone();

            let _campaign_context = campaign
                .clone()
                .validate(&config, campaign.channel.leader)
                .expect(
                    "Default development config should contain the dummy campaign.channel.token",
                );
        }
    }

    #[test]
    fn are_campaigns_validated() {
        let config = config::GANACHE_CONFIG.clone();

        // Validator not in campaign
        {
            let campaign = DUMMY_CAMPAIGN.clone();

            let validation_error = campaign
                .validate(&config, IDS[&GUARDIAN])
                .expect_err("Should trigger validation error");
            assert_eq!(
                Error::Validation(Validation::AdapterNotIncluded),
                validation_error,
            );
        }

        // active.to has passed
        {
            let mut campaign = DUMMY_CAMPAIGN.clone();
            campaign.active.to = Utc.ymd(2019, 1, 30).and_hms(0, 0, 0);

            let validation_error = campaign
                .validate(&config, IDS[&LEADER])
                .expect_err("Should trigger validation error");
            assert_eq!(
                Error::Validation(Validation::InvalidActiveTo),
                validation_error,
            );
        }

        // all_validators not listed
        {
            let campaign = DUMMY_CAMPAIGN.clone();
            let mut config = config::GANACHE_CONFIG.clone();
            config.validators_whitelist = vec![IDS[&LEADER], IDS[&GUARDIAN]];

            let validation_error = campaign
                .validate(&config, IDS[&LEADER])
                .expect_err("Should trigger validation error");
            assert_eq!(
                Error::Validation(Validation::UnlistedValidator),
                validation_error,
            );
        }

        // creator not listed
        {
            let campaign = DUMMY_CAMPAIGN.clone();
            let mut config = config::GANACHE_CONFIG.clone();
            config.creators_whitelist = vec![*PUBLISHER];

            let validation_error = campaign
                .validate(&config, IDS[&LEADER])
                .expect_err("Should trigger validation error");
            assert_eq!(
                Error::Validation(Validation::UnlistedCreator),
                validation_error,
            );
        }

        // token not listed
        {
            let mut campaign = DUMMY_CAMPAIGN.clone();
            campaign.channel.token = "0x0000000000000000000000000000000000000000"
                .parse::<Address>()
                .expect("Should parse");

            let validation_error = campaign
                .validate(&config, IDS[&LEADER])
                .expect_err("Should trigger validation error");
            assert_eq!(
                Error::Validation(Validation::UnlistedAsset),
                validation_error,
            );
        }

        // validator_fee < min_fee
        {
            let campaign = DUMMY_CAMPAIGN.clone();
            let mut config = config::GANACHE_CONFIG.clone();

            let mut token_info = config
                .chains
                .values_mut()
                .find_map(|chain_info| {
                    chain_info
                        .tokens
                        .values_mut()
                        .find(|token_info| token_info.address == campaign.channel.token)
                })
                .expect("Should find Dummy campaign.channel.token");
            token_info.min_validator_fee = BigNum::from_str("999999999999999999999999999999999999")
                .expect("Should parse BigNum");

            let validation_error = campaign
                .validate(&config, IDS[&LEADER])
                .expect_err("Should trigger validation error");
            assert_eq!(
                Error::Validation(Validation::MinimumValidatorFeeNotMet),
                validation_error,
            );
        }

        // let sum_fees = |validators: &Validators| -> UnifiedNum {
        //     validators
        //         .iter()
        //         .map(|validator| validator.fee)
        //         .sum::<Option<_>>()
        //         .expect("Validators sum of fees should not overflow")
        // };

        // // total_fee > budget
        // // budget = total_fee - 1
        // {
        //     let mut campaign = DUMMY_CAMPAIGN.clone();
        //     let campaign_token = config.find_chain_of(campaign.channel.token).unwrap().token;

        //     // makes the sum of all validator fees = 2 * min token units for deposit
        //     campaign.validators = {
        //         let new_validators = campaign
        //             .validators
        //             .iter()
        //             .map(|validator| {
        //                 let mut new_validator = validator.clone();
        //                 new_validator.fee = UnifiedNum::from_precision(
        //                     campaign_token.min_token_units_for_deposit.clone(),
        //                     campaign_token.precision.into(),
        //                 )
        //                 .expect("Should not overflow");

        //                 new_validator
        //             })
        //             .collect::<Vec<_>>();

        //         assert_eq!(
        //             2,
        //             new_validators.len(),
        //             "Dummy Campaign validators should always be 2 - a leader & a follower"
        //         );

        //         Validators::new((new_validators[0].clone(), new_validators[1].clone()))
        //     };

        //     campaign.budget = sum_fees(&campaign.validators) - UnifiedNum::from(1);

        //     let validation_error = campaign
        //         .validate(&config, IDS[&LEADER])
        //         .expect_err("Should trigger validation error");
        //     assert_eq!(
        //         Error::Validation(Validation::FeeConstraintViolated),
        //         validation_error,
        //     );
        // }

        // // total_fee = budget
        // {
        //     let mut campaign = DUMMY_CAMPAIGN.clone();

        //     let campaign_token = config.find_chain_of(campaign.channel.token).unwrap().token;

        //     // makes the sum of all validator fees = 2 * min token units for deposit
        //     campaign.validators = {
        //         let new_validators = campaign
        //             .validators
        //             .iter()
        //             .map(|validator| {
        //                 let mut new_validator = validator.clone();
        //                 new_validator.fee = UnifiedNum::from_precision(
        //                     campaign_token.min_token_units_for_deposit.clone(),
        //                     campaign_token.precision.into(),
        //                 )
        //                 .expect("Should not overflow");

        //                 new_validator
        //             })
        //             .collect::<Vec<_>>();

        //         assert_eq!(
        //             2,
        //             new_validators.len(),
        //             "Dummy Campaign validators should always be 2 - a leader & a follower"
        //         );

        //         Validators::new((new_validators[0].clone(), new_validators[1].clone()))
        //     };

        //     campaign.budget = sum_fees(&campaign.validators);

        //     let validation_error = campaign
        //         .validate(&config, IDS[&LEADER])
        //         .expect_err("Should trigger validation error");
        //     assert_eq!(
        //         Error::Validation(Validation::FeeConstraintViolated),
        //         validation_error,
        //     );
        // }

        // should validate
        {
            let campaign = DUMMY_CAMPAIGN.clone();
            let _campaign_context = campaign
                .validate(&config, IDS[&LEADER])
                .expect("Should pass validation");
        }
    }
}
