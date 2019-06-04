use crate::adapter::Adapter;
use crate::sanity::SanityChecker;

pub struct DummyAdapter {
    pub whoami: String,
}

impl SanityChecker for DummyAdapter {}

impl Adapter for DummyAdapter {
    fn whoami(&self) -> &str {
        &self.whoami
    }
}
