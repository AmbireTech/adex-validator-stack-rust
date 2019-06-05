use crate::adapter::{Adapter, Config};
use crate::sanity::SanityChecker;

pub struct DummyAdapter {
    pub config: Config,
}

impl SanityChecker for DummyAdapter {}

impl Adapter for DummyAdapter {
    fn config(&self) -> &Config {
        &self.config
    }
}
