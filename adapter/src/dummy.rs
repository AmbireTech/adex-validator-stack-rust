use crate::adapter::{Adapter, AdapterError, Config};
use crate::sanity::SanityChecker;
use hex::encode;
use std::collections::HashSet;
use std::hash::{Hash, Hasher};

#[derive(Eq, Debug)]
pub struct DummyParticipant {
    pub identity: String,
    pub token: String,
}

impl PartialEq for DummyParticipant {
    fn eq(&self, other: &Self) -> bool {
        self.identity == other.identity || self.token == other.token
    }
}

impl Hash for DummyParticipant {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.identity.hash(state);
        self.token.hash(state);
    }
}

pub struct DummyAdapter {
    pub config: Config,
    /// Dummy participants which will be used for
    /// Creator, Validator Leader, Validator Follower and etc.
    pub participants: HashSet<DummyParticipant>,
}

impl SanityChecker for DummyAdapter {}

impl Adapter for DummyAdapter {
    fn config(&self) -> &Config {
        &self.config
    }

    /// Example:
    ///
    /// ```
    /// use adapter::{ConfigBuilder, Adapter};
    /// use adapter::dummy::DummyAdapter;
    /// use std::collections::HashSet;
    ///
    /// let config = ConfigBuilder::new("identity").build();
    /// let adapter = DummyAdapter { config, participants: HashSet::new() };
    ///
    /// let actual = adapter.sign("abcdefghijklmnopqrstuvwxyz012345");
    /// let expected = "Dummy adapter signature for 6162636465666768696a6b6c6d6e6f707172737475767778797a303132333435 by identity";
    /// assert_eq!(expected, &actual);
    /// ```
    fn sign(&self, state_root: &str) -> String {
        format!(
            "Dummy adapter signature for {} by {}",
            encode(&state_root),
            &self.config.identity
        )
    }

    /// Example:
    ///
    /// ```
    /// use adapter::{ConfigBuilder, Adapter};
    /// use adapter::dummy::DummyAdapter;
    /// use std::collections::HashSet;
    ///
    /// let config = ConfigBuilder::new("identity").build();
    /// let adapter = DummyAdapter { config, participants: HashSet::new() };
    ///
    /// let signature = "Dummy adapter signature for 6162636465666768696a6b6c6d6e6f707172737475767778797a303132333435 by identity";
    /// assert!(adapter.verify("identity", "doesn't matter", signature) )
    /// ```
    fn verify(&self, signer: &str, _state_root: &str, signature: &str) -> bool {
        // select the `identity` and compare it to the signer
        // for empty string this will return array with 1 element - an empty string `[""]`
        match signature.rsplit(' ').take(1).next() {
            Some(from) => from == signer,
            None => false,
        }
    }

    fn get_auth(&self, validator: &str) -> Result<String, AdapterError<'_>> {
        match self
            .participants
            .iter()
            .find(|&participant| participant.identity == validator)
        {
            Some(participant) => Ok(participant.token.to_string()),
            None => Err(AdapterError::Authentication("Identity not found")),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::adapter::ConfigBuilder;

    #[test]
    fn sings_state_root_and_verifies_it() {
        let config = ConfigBuilder::new("identity").build();
        let adapter = DummyAdapter {
            config,
            participants: HashSet::new(),
        };

        let expected_signature = "Dummy adapter signature for 6162636465666768696a6b6c6d6e6f707172737475767778797a303132333435 by identity";
        let actual_signature = adapter.sign("abcdefghijklmnopqrstuvwxyz012345");

        assert_eq!(expected_signature, &actual_signature);

        assert!(adapter.verify("identity", "doesn't matter", &actual_signature))
    }
}
