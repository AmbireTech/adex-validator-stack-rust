use crate::adapter::{
    Adapter, AdapterError, AdapterFuture, BalanceRoot, ChannelId, Config, SignableStateRoot,
};
use crate::sanity::SanityChecker;

use futures::future::{err, ok, FutureExt};
use hex::encode;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;

use domain::validator::message::State;

#[derive(Debug)]
pub struct DummyParticipant {
    pub identity: String,
    pub token: String,
}

pub trait Hexable: AsRef<[u8]> {
    fn to_hex(&self) -> String {
        format!("0x{}", encode(&self))
    }
}

impl Hexable for ChannelId {}
impl Hexable for BalanceRoot {}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DummySignature(pub String);

impl fmt::Display for DummySignature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl<S: Into<String>> From<S> for DummySignature {
    fn from(value: S) -> Self {
        Self(value.into())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DummyStateRoot(pub String);

impl fmt::Display for DummyStateRoot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl<S: Into<String>> From<S> for DummyStateRoot {
    fn from(value: S) -> Self {
        Self(value.into())
    }
}

impl AsRef<[u8]> for DummyStateRoot {
    fn as_ref(&self) -> &[u8] {
        &self.0.as_ref()
    }
}
impl Hexable for DummyStateRoot {}

pub struct DummyAdapter<'a> {
    pub config: Config,
    /// Dummy participants which will be used for
    /// Creator, Validator Leader, Validator Follower and etc.
    pub participants: HashMap<&'a str, DummyParticipant>,
}

impl SanityChecker for DummyAdapter<'_> {}

impl State for DummyAdapter<'_> {
    type Signature = DummySignature;
    type StateRoot = DummyStateRoot;
}

impl<'a> Adapter for DummyAdapter<'a> {
    fn config(&self) -> &Config {
        &self.config
    }

    /// Example:
    ///
    /// ```
    /// # futures::executor::block_on(async {
    /// use adapter::{ConfigBuilder, Adapter};
    /// use adapter::dummy::{DummyAdapter, DummySignature};
    /// use std::collections::HashMap;
    ///
    /// let config = ConfigBuilder::new("identity").build();
    /// let adapter = DummyAdapter { config, participants: HashMap::new() };
    ///
    /// let actual = await!(adapter.sign(&"abcdefghijklmnopqrstuvwxyz012345".into())).unwrap();
    /// let expected = "Dummy adapter signature for 0x6162636465666768696a6b6c6d6e6f707172737475767778797a303132333435 by identity";
    /// assert_eq!(DummySignature::from(expected), actual);
    /// # });
    /// ```
    fn sign(&self, state_root: &Self::StateRoot) -> AdapterFuture<Self::Signature> {
        let signature = format!(
            "Dummy adapter signature for {} by {}",
            state_root.to_hex(),
            &self.config.identity
        );
        ok(signature.into()).boxed()
    }

    /// Example:
    ///
    /// ```
    /// # futures::executor::block_on(async {
    /// use adapter::{ConfigBuilder, Adapter};
    /// use adapter::dummy::DummyAdapter;
    /// use std::collections::HashMap;
    ///
    /// let config = ConfigBuilder::new("identity").build();
    /// let adapter = DummyAdapter { config, participants: HashMap::new() };
    ///
    /// let signature = "Dummy adapter signature for 0x6162636465666768696a6b6c6d6e6f707172737475767778797a303132333435 by identity";
    /// assert_eq!(Ok(true), await!(adapter.verify("identity", &"doesn't matter".into(), &signature.into())));
    /// # });
    /// ```
    fn verify(
        &self,
        signer: &str,
        _state_root: &Self::StateRoot,
        signature: &Self::Signature,
    ) -> AdapterFuture<bool> {
        // select the `identity` and compare it to the signer
        // for empty string this will return array with 1 element - an empty string `[""]`
        let is_same = match signature.0.rsplit(' ').take(1).next() {
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
    /// # futures::executor::block_on(async {
    /// use std::collections::HashMap;
    /// use adapter::dummy::{DummyParticipant, DummyAdapter};
    /// use adapter::{ConfigBuilder, Adapter};
    ///
    /// let mut participants = HashMap::new();
    /// participants.insert(
    ///     "identity_key",
    ///     DummyParticipant {
    ///         identity: "identity".to_string(),
    ///         token: "token".to_string(),
    ///     },
    /// );
    ///
    /// let adapter = DummyAdapter {
    ///     config: ConfigBuilder::new("identity").build(),
    ///     participants,
    /// };
    ///
    /// assert_eq!(Ok("token".to_string()), await!(adapter.get_auth("identity")));
    /// # });
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

    fn signable_state_root(
        channel_id: ChannelId,
        balance_root: BalanceRoot,
    ) -> SignableStateRoot<Self::StateRoot> {
        let state_root = format!(
            "Signable State Root for Adapter channel id {} with balance root {}",
            channel_id.to_hex(),
            balance_root.to_hex()
        );

        SignableStateRoot(state_root.into())
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

            let expected_signature = "Dummy adapter signature for 0x6162636465666768696a6b6c6d6e6f707172737475767778797a303132333435 by identity";
            let actual_signature = await!(adapter.sign(&"abcdefghijklmnopqrstuvwxyz012345".into()))
                .expect("Signing shouldn't fail");

            assert_eq!(DummySignature::from(expected_signature), actual_signature);

            let is_verified = await!(adapter.verify(
                "identity",
                &"doesn't matter".into(),
                &actual_signature.into()
            ));

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
