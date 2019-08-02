use std::{error, fmt};

use chrono::Utc;

use domain::channel::{SpecValidator, SpecValidators};
use domain::{Asset, Channel, ValidatorId};

use crate::adapter::Config;
use std::convert::TryFrom;

pub trait SanityChecker {
    fn check(config: &Config, channel: &Channel) -> Result<(), SanityError> {
        let identity =
            ValidatorId::try_from(config.identity.as_str()).expect("Identity failed to be created");
        let adapter_channel_validator = match channel.spec.validators.find(&identity) {
            // check if the channel validators include our adapter identity
            SpecValidator::None => return Err(SanityError::AdapterNotIncluded),
            SpecValidator::Leader(validator) | SpecValidator::Follower(validator) => validator,
        };

        if channel.valid_until < Utc::now() {
            return Err(SanityError::PassedValidUntil);
        }

        if !all_validators_listed(&channel.spec.validators, &config.validators_whitelist) {
            return Err(SanityError::UnlistedValidator);
        }

        if !creator_listed(&channel, &config.creators_whitelist) {
            return Err(SanityError::UnlistedCreator);
        }

        if !asset_listed(&channel, &config.assets_whitelist) {
            return Err(SanityError::UnlistedAsset);
        }

        if channel.deposit_amount < config.minimal_deposit {
            return Err(SanityError::MinimumDepositNotMet);
        }

        if adapter_channel_validator.fee < config.minimal_fee {
            return Err(SanityError::MinimumValidatorFeeNotMet);
        }

        Ok(())
    }
}

fn all_validators_listed(validators: &SpecValidators, whitelist: &[String]) -> bool {
    if whitelist.is_empty() {
        true
    } else {
        let found_validators = whitelist
            .iter()
            .filter(|&allowed| {
                allowed == validators.leader().id.as_ref()
                    || allowed == validators.follower().id.as_ref()
            })
            // this will ensure that if we find the 2 validators earlier
            // we don't go over the other values of the whitelist
            .take(2);
        // the found validators should be exactly 2, if they are not, then 1 or 2 are missing
        found_validators.count() == 2
    }
}

fn creator_listed(channel: &Channel, whitelist: &[String]) -> bool {
    // if the list is empty, return true, as we don't have a whitelist to restrict us to
    // or if we have a list, check if it includes the `channel.creator`
    whitelist.is_empty() || whitelist.iter().any(|allowed| allowed == &channel.creator)
}

fn asset_listed(channel: &Channel, whitelist: &[Asset]) -> bool {
    // if the list is empty, return true, as we don't have a whitelist to restrict us to
    // or if we have a list, check if it includes the `channel.deposit_asset`
    whitelist.is_empty()
        || whitelist
            .iter()
            .any(|allowed| allowed == &channel.deposit_asset)
}

#[derive(Debug, PartialEq, Eq)]
pub enum SanityError {
    /// When the Adapter address is not listed in the `channel.spec.validators`
    /// which in terms means, that the adapter shouldn't handle this Channel
    AdapterNotIncluded,
    /// when `channel.valid_until` has passed (< now), the channel should be handled
    PassedValidUntil,
    UnlistedValidator,
    UnlistedCreator,
    UnlistedAsset,
    MinimumDepositNotMet,
    MinimumValidatorFeeNotMet,
}

impl fmt::Display for SanityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Sanity error",)
    }
}

impl error::Error for SanityError {
    fn cause(&self) -> Option<&dyn error::Error> {
        None
    }
}

#[cfg(test)]
mod test {
    use time::Duration;

    use domain::channel::fixtures::{get_channel_spec, ValidatorsOption};
    use domain::fixtures::{get_channel, get_validator};

    use crate::adapter::ConfigBuilder;

    use super::*;

    pub struct DummySanityChecker {}
    impl SanityChecker for DummySanityChecker {}

    #[test]
    fn sanity_check_disallows_channels_without_current_adapter() {
        let channel = get_channel("channel_1", &None, None);
        let config = ConfigBuilder::new("non_existent_validator").build();
        assert_eq!(
            Err(SanityError::AdapterNotIncluded),
            DummySanityChecker::check(&config, &channel)
        )
    }

    #[test]
    fn sanity_check_disallows_channels_with_passed_valid_until() {
        let passed_valid_until = Utc::now() - Duration::seconds(1);
        let channel = get_channel("channel_1", &Some(passed_valid_until), None);

        let identity = channel.spec.validators.leader().id.clone();
        let config = ConfigBuilder::new(identity.as_ref()).build();

        assert_eq!(
            Err(SanityError::PassedValidUntil),
            DummySanityChecker::check(&config, &channel)
        )
    }

    #[test]
    fn sanity_check_disallows_channels_with_unlisted_in_whitelist_validators() {
        let channel = get_channel("channel_1", &None, None);

        // as identity use the leader, otherwise we won't pass the AdapterNotIncluded check
        let identity = channel.spec.validators.leader().id.clone();
        let config = ConfigBuilder::new(identity.as_ref())
            .set_validators_whitelist(&["my validator"])
            .build();

        // make sure we don't use the leader or follower validators as a whitelisted validator
        assert_ne!(
            identity.as_ref(),
            "my validator",
            "The whitelisted validator and the leader have the same id"
        );
        assert_ne!(
            channel.spec.validators.follower().id.as_ref(),
            "my validator",
            "The whitelisted validator and the follower have the same id"
        );

        assert_eq!(
            Err(SanityError::UnlistedValidator),
            DummySanityChecker::check(&config, &channel)
        )
    }

    #[test]
    fn sanity_check_disallows_channels_with_unlisted_creator() {
        let channel = get_channel("channel_1", &None, None);

        // as identity use the leader, otherwise we won't pass the AdapterNotIncluded check
        let identity = channel.spec.validators.leader().id.clone();
        let config = ConfigBuilder::new(identity.as_ref())
            .set_creators_whitelist(&["creator"])
            .build();

        assert_ne!(
            &channel.creator, "creator",
            "The channel creator should be different than the whitelisted creator"
        );

        assert_eq!(
            Err(SanityError::UnlistedCreator),
            DummySanityChecker::check(&config, &channel)
        )
    }

    #[test]
    fn sanity_check_disallows_channels_with_unlisted_asset() {
        let channel = get_channel("channel_1", &None, None);

        // as identity use the leader, otherwise we won't pass the AdapterNotIncluded check
        let identity = channel.spec.validators.leader().id.clone();
        let config = ConfigBuilder::new(identity.as_ref())
            .set_assets_whitelist(&["ASSET".into()])
            .build();

        assert_ne!(
            &channel.deposit_asset,
            &"ASSET".into(),
            "The channel deposit_asset should be different than the whitelisted asset"
        );

        assert_eq!(
            Err(SanityError::UnlistedAsset),
            DummySanityChecker::check(&config, &channel)
        )
    }

    #[test]
    fn sanity_check_disallows_channel_deposit_less_than_minimum_deposit() {
        let channel = get_channel("channel_1", &None, None);

        // as identity use the leader, otherwise we won't pass the AdapterNotIncluded check
        let identity = channel.spec.validators.leader().id.clone();
        let config = ConfigBuilder::new(identity.as_ref())
            // set the minimum deposit to the `channel.deposit_amount + 1`
            .set_minimum_deposit(&channel.deposit_amount + &1.into())
            .build();

        assert_eq!(
            Err(SanityError::MinimumDepositNotMet),
            DummySanityChecker::check(&config, &channel)
        )
    }

    #[test]
    fn sanity_check_disallows_validator_fee_less_than_minimum_fee() {
        let channel = get_channel("channel_1", &None, None);

        let leader = channel.spec.validators.leader();
        // as identity use the leader, otherwise we won't pass the AdapterNotIncluded check
        let identity = leader.id.clone();
        let config = ConfigBuilder::new(identity.as_ref())
            // set the minimum deposit to the `leader.fee + 1`
            .set_minimum_fee(&leader.fee + &1.into())
            .build();

        assert_eq!(
            Err(SanityError::MinimumValidatorFeeNotMet),
            DummySanityChecker::check(&config, &channel)
        )
    }

    #[test]
    fn sanity_check_allows_for_valid_values() {
        let validators = [
            get_validator("my leader", Some(10.into())),
            get_validator("my follower", Some(15.into())),
        ];
        let spec = get_channel_spec(ValidatorsOption::SpecValidators(validators.into()));
        let channel = get_channel("channel_1", &None, Some(spec));

        // as identity use the leader, otherwise we won't pass the AdapterNotIncluded check
        let config = ConfigBuilder::new("my leader")
            .set_validators_whitelist(&["my leader", "my follower"])
            .set_creators_whitelist(&[&channel.creator])
            .set_assets_whitelist(&[channel.deposit_asset.clone()])
            // set the minimum deposit to the `channel.deposit_amount - 1`
            .set_minimum_deposit(&channel.deposit_amount - &1.into())
            // set the minimum fee to the `leader.fee - 1`, i.e. `10 - 1 = 9`
            .set_minimum_fee(9.into())
            .build();

        assert!(DummySanityChecker::check(&config, &channel).is_ok())
    }
}
