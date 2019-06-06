use crate::adapter::{Adapter, Config};
use crate::sanity::SanityChecker;
use hex::encode;

pub struct DummyAdapter {
    pub config: Config,
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
    ///
    /// let config = ConfigBuilder::new("identity").build();
    /// let adapter = DummyAdapter { config };
    ///
    /// let actual = adapter.sign("abcdefghijklmnopqrstuvwxyz012345");
    /// assert_eq!("Dummy adapter signature for 6162636465666768696a6b6c6d6e6f707172737475767778797a303132333435 by identity", &actual);
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
    ///
    /// let config = ConfigBuilder::new("identity").build();
    /// let adapter = DummyAdapter { config };
    ///
    /// assert!(adapter.verify("identity", "doesn't matter", "Dummy adapter signature for 6162636465666768696a6b6c6d6e6f707172737475767778797a303132333435 by identity") )
    /// ```
    fn verify(&self, signer: &str, _state_root: &str, signature: &str) -> bool {
        // select the `identity` and compare it to the signer
        // for empty string this will return array with 1 element - an empty string `[""]`
        match signature.rsplit(' ').take(1).next() {
            Some(from) => from == signer,
            None => false,
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
        let adapter = DummyAdapter { config };

        let expected_signature = "Dummy adapter signature for 6162636465666768696a6b6c6d6e6f707172737475767778797a303132333435 by identity";
        let actual_signature = adapter.sign("abcdefghijklmnopqrstuvwxyz012345");

        assert_eq!(expected_signature, &actual_signature);

        assert!(adapter.verify("identity", "doesn't matter", &actual_signature))
    }
}
