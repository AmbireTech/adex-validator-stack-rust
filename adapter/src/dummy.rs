use std::collections::HashMap;

use futures::future::{err, FutureExt, ok};
use hex::encode;

use domain::validator::message::State;

use crate::adapter::{Adapter, AdapterError, AdapterFuture, Config};
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

impl<'a> State for DummyAdapter<'a> {
    type Signature = &'a str;
    type StateRoot = &'a str;
}

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
    /// futures::executor::block_on(async {
    ///     let config = ConfigBuilder::new("identity").build();
    ///     let adapter = DummyAdapter { config, participants: HashMap::new() };
    ///
    ///     let actual = await!(adapter.sign("abcdefghijklmnopqrstuvwxyz012345")).unwrap();
    ///     let expected = "Dummy adapter signature for 6162636465666768696a6b6c6d6e6f707172737475767778797a303132333435 by identity";
    ///     assert_eq!(expected, &actual);
    /// });
    /// ```
    fn sign(&self, state_root: &str) -> AdapterFuture<String> {
        let signature = format!(
            "Dummy adapter signature for {} by {}",
            encode(&state_root),
            &self.config.identity
        );
        ok(signature).boxed()
    }

    /// Example:
    ///
    /// ```
    /// use adapter::{ConfigBuilder, Adapter};
    /// use adapter::dummy::DummyAdapter;
    /// use std::collections::HashMap;
    ///
    /// futures::executor::block_on(async {
    ///     let config = ConfigBuilder::new("identity").build();
    ///     let adapter = DummyAdapter { config, participants: HashMap::new() };
    ///
    ///     let signature = "Dummy adapter signature for 6162636465666768696a6b6c6d6e6f707172737475767778797a303132333435 by identity";
    ///     assert_eq!(Ok(true), await!(adapter.verify("identity", "doesn't matter", signature)));
    /// });
    /// ```
    fn verify(
        &self,
        signer: &str,
        _state_root: Self::StateRoot,
        signature: Self::Signature,
    ) -> AdapterFuture<bool> {
        // select the `identity` and compare it to the signer
        // for empty string this will return array with 1 element - an empty string `[""]`
        let is_same = match signature.rsplit(' ').take(1).next() {
            Some(from) => from == signer,
            None => false,
        };

        ok(is_same).boxed()
    }

    /// Finds the auth. token in the HashMap of DummyParticipants if exists
    ///
    /// Example:
    ///
    /// ```
    /// use std::collections::HashMap;
    /// use adapter::dummy::{DummyParticipant, DummyAdapter};
    /// use adapter::{ConfigBuilder, Adapter};
    ///
    /// futures::executor::block_on(async {
    ///    let mut participants = HashMap::new();
    ///    participants.insert(
    ///        "identity_key",
    ///        DummyParticipant {
    ///            identity: "identity".to_string(),
    ///            token: "token".to_string(),
    ///        },
    ///    );
    ///
    ///    let adapter = DummyAdapter {
    ///        config: ConfigBuilder::new("identity").build(),
    ///        participants,
    ///    };
    ///
    ///    assert_eq!(Ok("token".to_string()), await!(adapter.get_auth("identity")));
    /// });
    /// ```
    fn get_auth(&self, validator: &str) -> AdapterFuture<String> {
        let participant = self
            .participants
            .iter()
            .find(|&(_, participant)| participant.identity == validator);
        let future = match participant {
            Some((_, participant)) => ok(participant.token.to_string()),
            None => err(AdapterError::Authentication(
                "Identity not found".to_string(),
            )),
        };

        future.boxed()
    }
}

#[cfg(test)]
mod test {
    use crate::adapter::ConfigBuilder;

    use super::*;

    #[test]
    fn dummy_adapter_sings_state_root_and_verifies_it() {
        futures::executor::block_on(async {
            let config = ConfigBuilder::new("identity").build();
            let adapter = DummyAdapter {
                config,
                participants: HashMap::new(),
            };

            let expected_signature = "Dummy adapter signature for 6162636465666768696a6b6c6d6e6f707172737475767778797a303132333435 by identity";
            let actual_signature = await!(adapter.sign("abcdefghijklmnopqrstuvwxyz012345"))
                .expect("Signing shouldn't fail");

            assert_eq!(expected_signature, &actual_signature);

            let is_verified =
                await!(adapter.verify("identity", "doesn't matter", &actual_signature));

            assert_eq!(Ok(true), is_verified);
        });
    }

    #[test]
    fn get_auth_with_empty_participators() {
        futures::executor::block_on(async {
            let adapter = DummyAdapter {
                config: ConfigBuilder::new("identity").build(),
                participants: HashMap::new(),
            };

            assert_eq!(
                Err(AdapterError::Authentication(
                    "Identity not found".to_string()
                )),
                await!(adapter.get_auth("non-existing"))
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
                Err(AdapterError::Authentication(
                    "Identity not found".to_string()
                )),
                await!(adapter.get_auth("non-existing"))
            );
        });
    }
}
