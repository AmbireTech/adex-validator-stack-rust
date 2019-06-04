use domain::Channel;
use std::{error, fmt};

pub trait SanityChecker {
    fn check(adapter_address: &String, channel: &Channel) -> Result<(), SanityError> {
        let channel_has_adapter = channel
            .spec
            .validators
            .iter()
            .find(|&validator| &validator.id.to_lowercase() == &adapter_address.to_lowercase())
            .is_some();

        if channel_has_adapter {
            Ok(())
        } else {
            Err(SanityError {})
        }
    }
}

#[derive(Debug)]
pub struct SanityError {}

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

pub trait Adapter: SanityChecker {
    fn whoami(&self) -> &String;

    fn validate_channel(&self, channel: &Channel) -> bool {
        Self::check(&self.whoami(), &channel).is_ok()
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use domain::fixtures::{
        get_channel, get_channel_spec, get_validator, get_validators, ValidatorsOption,
    };
    use domain::ValidatorDesc;

    pub struct DummyAdapter {
        whoami: String,
    }
    impl SanityChecker for DummyAdapter {}

    impl Adapter for DummyAdapter {
        fn whoami(&self) -> &String {
            &self.whoami
        }
    }

    #[test]
    fn sanity_check_disallows_channels_without_current_adapter() {
        let adapter = DummyAdapter {
            whoami: "non_existent_validator".to_string(),
        };
        let channel = get_channel("channel_1", &None, None);

        assert!(!adapter.validate_channel(&channel))
    }

    #[test]
    fn sanity_check_allows_channels_with_current_adapter() {
        let adapter = DummyAdapter {
            whoami: "my validator".to_string(),
        };
        let mut validators = get_validators(2, None);
        validators.push(get_validator("my validator"));

        let spec = get_channel_spec("spec", ValidatorsOption::Some(validators));

        let channel = get_channel("channel_1", &None, Some(spec));

        assert!(adapter.validate_channel(&channel))
    }
}
