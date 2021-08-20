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
