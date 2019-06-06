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

    fn sign(&self, state_root: &str) -> String {
        format!(
            "Dummy adapter signature for {} by {}",
            encode(&state_root),
            &self.config.identity
        )
    }

    /// Sample signature
    /// `Dummy adapter for 6def5a300acb6fcaa0dab3a41e9d6457b5147a641e641380f8cc4bf5308b16fe by awesomeLeader`
    fn verify(&self, signer: &str, _state_root: &str, signature: &str) -> bool {
        // select the `awesomeLeader` and compare it to the signer
        // for empty string this will return array with 1 element - an empty string `[""]`
        match signature.rsplit(' ').take(1).next() {
            Some(from) => from == signer,
            None => false,
        }
    }
}
