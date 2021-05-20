use crate::{campaign::Validators, config::Config, Address, Campaign, UnifiedNum, ValidatorId};
use chrono::Utc;
use std::cmp::PartialEq;
use thiserror::Error;

pub trait Validator {
    fn validate(
        &self,
        config: &Config,
        validator_identity: &ValidatorId,
    ) -> Result<Validation, Error>;
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Validation {
    Ok,
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
}

impl Validator for Campaign {
    fn validate(
        &self,
        config: &Config,
        validator_identity: &ValidatorId,
    ) -> Result<Validation, Error> {
        // check if the channel validators include our adapter identity
        let whoami_validator = match self.find_validator(validator_identity) {
            Some(role) => role.validator(),
            None => return Ok(Validation::AdapterNotIncluded),
        };

        if self.active.to < Utc::now() {
            return Ok(Validation::InvalidActiveTo);
        }

        if !all_validators_listed(&self.validators, &config.validators_whitelist) {
            return Ok(Validation::UnlistedValidator);
        }

        if !creator_listed(&self, &config.creators_whitelist) {
            return Ok(Validation::UnlistedCreator);
        }

        if !asset_listed(&self, &config.token_address_whitelist) {
            return Ok(Validation::UnlistedAsset);
        }

        // TODO AIP#61: Use configuration to check the minimum deposit of the token!
        if self.budget < UnifiedNum::from(500) {
            return Ok(Validation::MinimumDepositNotMet);
        }

        // TODO AIP#61: Use Configuration to check the minimum validator fee of the token!
        if whoami_validator.fee < UnifiedNum::from(100) {
            return Ok(Validation::MinimumValidatorFeeNotMet);
        }

        let total_validator_fee: UnifiedNum = self
            .validators
            .iter()
            .map(|v| &v.fee)
            .sum::<Option<_>>()
            // on overflow return an error
            .ok_or(Error::FeeSumOverflow)?;

        if total_validator_fee >= self.budget {
            return Ok(Validation::FeeConstraintViolated);
        }

        Ok(Validation::Ok)
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

pub fn asset_listed(campaign: &Campaign, whitelist: &[String]) -> bool {
    // if the list is empty, return true, as we don't have a whitelist to restrict us to
    // or if we have a list, check if it includes the `channel.deposit_asset`
    whitelist.is_empty()
        || whitelist
            .iter()
            .any(|allowed| allowed == &campaign.channel.token.to_string())
}
