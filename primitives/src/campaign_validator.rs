use crate::{
    campaign::Validators,
    config::{Config, TokenInfo},
    Address, Campaign, UnifiedNum, ValidatorId,
};
use chrono::Utc;
use std::{cmp::PartialEq, collections::HashMap};
use thiserror::Error;

pub trait Validator {
    fn validate(&self, config: &Config, validator_identity: &ValidatorId) -> Result<(), Error>;
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
    MinimumDepositNotMet,
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
    fn validate(&self, config: &Config, validator_identity: &ValidatorId) -> Result<(), Error> {
        // check if the channel validators include our adapter identity
        let whoami_validator = match self.find_validator(validator_identity) {
            Some(role) => role.into_inner(),
            None => return Err(Validation::AdapterNotIncluded.into()),
        };

        if self.active.to < Utc::now() {
            return Err(Validation::InvalidActiveTo.into());
        }

        if !all_validators_listed(&self.validators, &config.validators_whitelist) {
            return Err(Validation::UnlistedValidator.into());
        }

        if !creator_listed(self, &config.creators_whitelist) {
            return Err(Validation::UnlistedCreator.into());
        }

        // Check if the token is listed in the Configuration
        let token_info = config
            .token_address_whitelist
            .get(&self.channel.token)
            .ok_or(Validation::UnlistedAsset)?;

        // Check if the campaign budget is above the minimum deposit configured
        if self.budget.to_precision(token_info.precision.get())
            < token_info.min_token_units_for_deposit
        {
            return Err(Validation::MinimumDepositNotMet.into());
        }

        // Check if the validator fee is greater than the minimum configured fee
        if whoami_validator
            .fee
            .to_precision(token_info.precision.get())
            < token_info.min_validator_fee
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

        Ok(())
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

pub fn asset_listed(campaign: &Campaign, whitelist: &HashMap<Address, TokenInfo>) -> bool {
    // if the list is empty, return true, as we don't have a whitelist to restrict us to
    // or if we have a list, check if it includes the `channel.deposit_asset`
    whitelist.is_empty()
        || whitelist
            .keys()
            .any(|allowed| allowed == &campaign.channel.token)
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::config::configuration;
    use crate::util::tests::prep_db::{
        ADDRESSES, DUMMY_CAMPAIGN, DUMMY_VALIDATOR_FOLLOWER, DUMMY_VALIDATOR_LEADER, IDS, TOKENS,
    };
    use crate::BigNum;
    use chrono::{TimeZone, Utc};
    use std::num::NonZeroU8;
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
        let are_listed = all_validators_listed(&validators, &[IDS["user"], IDS["tester"]]);
        assert!(!are_listed);
        // one validator listed
        let are_listed =
            all_validators_listed(&validators, &[IDS["user"], IDS["tester"], IDS["leader"]]);
        assert!(!are_listed);
        // both validators lister
        let are_listed = all_validators_listed(
            &validators,
            &[IDS["user"], IDS["tester"], IDS["leader"], IDS["follower"]],
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
        let is_listed = creator_listed(&campaign, &[ADDRESSES["tester"]]);
        assert!(!is_listed);

        // listed
        let is_listed = creator_listed(&campaign, &[ADDRESSES["tester"], campaign.creator]);
        assert!(is_listed);
    }

    #[test]
    fn is_asset_listed() {
        let campaign = DUMMY_CAMPAIGN.clone();

        let mut assets = HashMap::new();
        // empty hashmap
        let is_listed = asset_listed(&campaign, &assets);
        assert!(is_listed);

        // not listed

        assets.insert(
            TOKENS["USDC"],
            TokenInfo {
                min_token_units_for_deposit: BigNum::from(0),
                min_validator_fee: BigNum::from(0),
                precision: NonZeroU8::new(6).expect("should create NonZeroU8"),
            },
        );
        let is_listed = asset_listed(&campaign, &assets);
        assert!(!is_listed);

        // listed
        assets.insert(
            TOKENS["DAI"],
            TokenInfo {
                min_token_units_for_deposit: BigNum::from(0),
                min_validator_fee: BigNum::from(0),
                precision: NonZeroU8::new(18).expect("should create NonZeroU8"),
            },
        );
        let is_listed = asset_listed(&campaign, &assets);
        assert!(is_listed);
    }

    #[test]
    fn are_campaigns_validated() {
        let config = configuration("development", None).expect("Should get Config");

        // Validator not in campaign
        {
            let campaign = DUMMY_CAMPAIGN.clone();
            let is_validated = campaign.validate(&config, &IDS["tester"]);
            assert!(matches!(
                is_validated,
                Err(Error::Validation(Validation::AdapterNotIncluded))
            ));
        }

        // active.to has passed
        {
            let mut campaign = DUMMY_CAMPAIGN.clone();
            campaign.active.to = Utc.ymd(2019, 1, 30).and_hms(0, 0, 0);
            let is_validated = campaign.validate(&config, &IDS["leader"]);
            assert!(matches!(
                is_validated,
                Err(Error::Validation(Validation::InvalidActiveTo))
            ));
        }

        // all_validators not listed
        {
            let campaign = DUMMY_CAMPAIGN.clone();
            let mut config = configuration("development", None).expect("Should get Config");
            config.validators_whitelist = vec![IDS["leader"], IDS["tester"]];

            let is_validated = campaign.validate(&config, &IDS["leader"]);
            assert!(matches!(
                is_validated,
                Err(Error::Validation(Validation::UnlistedValidator))
            ));
        }

        // creator not listed
        {
            let campaign = DUMMY_CAMPAIGN.clone();
            let mut config = configuration("development", None).expect("Should get Config");
            config.creators_whitelist = vec![ADDRESSES["tester"]];

            let is_validated = campaign.validate(&config, &IDS["leader"]);
            assert!(matches!(
                is_validated,
                Err(Error::Validation(Validation::UnlistedCreator))
            ));
        }

        // token not listed
        {
            let mut campaign = DUMMY_CAMPAIGN.clone();
            campaign.channel.token = "0x0000000000000000000000000000000000000000"
                .parse::<Address>()
                .expect("Should parse");

            let is_validated = campaign.validate(&config, &IDS["leader"]);
            assert!(matches!(
                is_validated,
                Err(Error::Validation(Validation::UnlistedAsset))
            ));
        }

        // budget < min_deposit
        {
            let mut campaign = DUMMY_CAMPAIGN.clone();
            campaign.budget = UnifiedNum::from_u64(0);

            let is_validated = campaign.validate(&config, &IDS["leader"]);
            assert!(matches!(
                is_validated,
                Err(Error::Validation(Validation::MinimumDepositNotMet))
            ));
        }

        // validator_fee < min_fee
        {
            let campaign = DUMMY_CAMPAIGN.clone();
            let mut config = configuration("development", None).expect("Should get Config");

            config.token_address_whitelist.insert(
                TOKENS["DAI"],
                TokenInfo {
                    min_token_units_for_deposit: BigNum::from(0),
                    min_validator_fee: BigNum::from_str("999999999999999999999999999999999999")
                        .expect("should get BigNum"),
                    precision: NonZeroU8::new(18).expect("should create NonZeroU8"),
                },
            );

            let is_validated = campaign.validate(&config, &IDS["leader"]);
            assert!(matches!(
                is_validated,
                Err(Error::Validation(Validation::MinimumValidatorFeeNotMet))
            ));
        }

        // total_fee > budget
        {
            let mut campaign = DUMMY_CAMPAIGN.clone();
            campaign.budget = UnifiedNum::from_u64(150); // both fees are 100, so this won't cover them

            let is_validated = campaign.validate(&config, &IDS["leader"]);
            assert!(matches!(
                is_validated,
                Err(Error::Validation(Validation::FeeConstraintViolated))
            ));
        }

        // total_fee = budget
        {
            let mut campaign = DUMMY_CAMPAIGN.clone();
            campaign.budget = UnifiedNum::from_u64(200);

            let is_validated = campaign.validate(&config, &IDS["leader"]);
            assert!(matches!(
                is_validated,
                Err(Error::Validation(Validation::FeeConstraintViolated))
            ));
        }
        // should validate
        {
            let campaign = DUMMY_CAMPAIGN.clone();
            let is_validated = campaign.validate(&config, &IDS["leader"]);
            assert!(is_validated.is_ok());
        }
    }
}
