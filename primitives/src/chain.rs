use serde::{Deserialize, Serialize};
use std::fmt;

use crate::util::ApiUrl;

#[derive(Serialize, Deserialize, Hash, Clone, Copy, Eq, PartialEq)]
#[serde(transparent)]
pub struct ChainId(u32);

impl ChainId {
    /// # Panics:
    ///
    /// If `id` is `0`.
    pub fn new(id: u32) -> Self {
        assert!(id != 0);

        Self(id)
    }
}

impl From<u32> for ChainId {
    fn from(id: u32) -> Self {
        Self::new(id)
    }
}

impl fmt::Debug for ChainId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ChainId({})", self.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Chain {
    pub chain_id: ChainId,
    pub rpc: ApiUrl,
}