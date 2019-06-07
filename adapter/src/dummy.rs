use std::collections::HashMap;

use hex::encode;

use crate::adapter::{Adapter, AdapterError, Config};
use crate::sanity::SanityChecker;

#[derive(Debug)]
pub struct DummyParticipant {
    pub identity: String,
    pub token: String,
}

pub struct DummyAdapter<'a> {
    pub config: Config,
    /// Dummy participants which will be used for
    /// Creator, Validator Leader, Validator Follower and etc.
    pub participants: HashMap<&'a str, DummyParticipant>,
}

impl SanityChecker for DummyAdapter<'_> {}

impl Adapter for DummyAdapter<'_> {
    fn config(&self) -> &Config {
        &self.config
    }

    /// Example:
    ///
    /// ```
    /// use adapter::{ConfigBuilder, Adapter};
    /// use adapter::dummy::DummyAdapter;
    /// use std::collections::HashMap;
    ///
    /// let config = ConfigBuilder::new("identity").build();
    /// let adapter = DummyAdapter { config, participants: HashMap::new() };
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
    /// use std::collections::HashMap;
    ///
    /// let config = ConfigBuilder::new("identity").build();
    /// let adapter = DummyAdapter { config, participants: HashMap::new() };
    ///
    /// let signature = "Dummy adapter signature for 6162636465666768696a6b6c6d6e6f707172737475767778797a303132333435 by identity";
    /// assert!(adapter.verify("identity", "doesn't matter", signature));
    /// ```
    fn verify(&self, signer: &str, _state_root: &str, signature: &str) -> bool {
        // select the `identity` and compare it to the signer
        // for empty string this will return array with 1 element - an empty string `[""]`
        match signature.rsplit(' ').take(1).next() {
            Some(from) => from == signer,
            None => false,
        }
    }

    /// Finds the auth. token in the HashMap of DummyParticipants if exists
    /// Example:
    ///
    /// ```
    /// use std::collections::HashMap;
    /// use adapter::dummy::{DummyParticipant, DummyAdapter};
    /// use adapter::{ConfigBuilder, Adapter};
    ///
    /// let mut participants = HashMap::new();
    /// participants.insert(
    ///    "identity_key",
    ///    DummyParticipant {
    ///        identity: "identity".to_string(),
    ///        token: "token".to_string(),
    ///    },
    /// );
    ///
    ///let adapter = DummyAdapter {
    ///    config: ConfigBuilder::new("identity").build(),
    ///    participants,
    ///};
    ///
    ///assert_eq!(Ok("token".to_string()), adapter.get_auth("identity"));
    /// ```
    fn get_auth(&self, validator: &str) -> Result<String, AdapterError<'_>> {
        match self
            .participants
            .iter()
            .find(|&(_, participant)| participant.identity == validator)
        {
            Some((_, participant)) => Ok(participant.token.to_string()),
            None => Err(AdapterError::Authentication("Identity not found")),
        }
    }
}

#[cfg(test)]
mod test {
    use crate::adapter::ConfigBuilder;

    use super::*;

    #[test]
    fn dummy_adapter_sings_state_root_and_verifies_it() {
        let config = ConfigBuilder::new("identity").build();
        let adapter = DummyAdapter {
            config,
            participants: HashMap::new(),
        };

        let expected_signature = "Dummy adapter signature for 6162636465666768696a6b6c6d6e6f707172737475767778797a303132333435 by identity";
        let actual_signature = adapter.sign("abcdefghijklmnopqrstuvwxyz012345");

        assert_eq!(expected_signature, &actual_signature);

        assert!(adapter.verify("identity", "doesn't matter", &actual_signature))
    }

    #[test]
    fn get_auth_with_empty_participators() {
        let adapter = DummyAdapter {
            config: ConfigBuilder::new("identity").build(),
            participants: HashMap::new(),
        };

        assert_eq!(
            AdapterError::Authentication("Identity not found"),
            adapter.get_auth("non-existing").unwrap_err()
        );

        let mut participants = HashMap::new();
        participants.insert(
            "identity_key",
            DummyParticipant {
                identity: "identity".to_string(),
                token: "token".to_string(),
            },
        );
        let adapter = DummyAdapter {
            config: ConfigBuilder::new("identity").build(),
            participants,
        };

        assert_eq!(
            AdapterError::Authentication("Identity not found"),
            adapter.get_auth("non-existing").unwrap_err()
        );
    }
}
