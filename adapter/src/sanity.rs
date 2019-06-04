use domain::Channel;
use std::{error, fmt};

pub trait SanityChecker {
    fn check(adapter_address: &String, channel: &Channel) -> Result<(), SanityError> {
        let channel_has_adapter = channel.spec.validators.find(adapter_address);

        if channel_has_adapter.is_some() {
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

#[cfg(test)]
mod test {
    use domain::fixtures::{get_channel, get_channel_spec, get_validator};

    use super::*;

    pub struct DummySanityChecker {}
    impl SanityChecker for DummySanityChecker {}

    #[test]
    fn sanity_check_disallows_channels_without_current_adapter() {
        let channel = get_channel("channel_1", &None, None);

        assert!(DummySanityChecker::check(&"non_existent_validator".to_string(), &channel).is_err())
    }

    #[test]
    fn sanity_check_allows_channels_with_current_adapter() {
        let spec_validators = [get_validator("validator 1"), get_validator("my validator")].into();

        let spec = get_channel_spec("spec", Some(spec_validators));

        let channel = get_channel("channel_1", &None, Some(spec));

        assert!(DummySanityChecker::check(&"my validator".to_string(), &channel).is_ok())
    }
}
