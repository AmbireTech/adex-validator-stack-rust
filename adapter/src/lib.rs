pub use error::Error;
pub use adapter::{LockedC, UnlockedC};

/// only re-export types from the `primitives` crate that are being used by the [`crate::Adapter`].
pub mod primitives {
    use serde::{Serialize, Deserialize};

    /// Re-export all the types used from the [`primitives`] crate.
    pub use ::primitives::{Address, Channel, ValidatorId, BigNum};

    pub type Deposit = ::primitives::Deposit<primitives::BigNum>;

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Session {
        pub era: i64,
        pub uid: Address,
    }
}

/// Re-export of the [`crate::client`] traits.
pub mod prelude {
    /// Re-export traits used for working with the [`crate::Adapter`].
    pub use crate::client::{LockedClient, UnlockedClient, Unlockable};
}

/// The [`Adapter`] trait and it's states
/// Used for communication with underlying implementation.
/// 2 Adapters are available in this crate:
/// - [`crate::ethereum::Adapter`] and it's client implementation [`crate::Ethereum`] for chains compatible with EVM.
/// - [`crate::dummy::Adapter`] and it's client implementation [`crate::Dummy`] for testing.
mod adapter;
pub mod client;
mod error;

pub use adapter::{Adapter, Locked, Unlocked};

/// Dummy testing client for [`Adapter`]
pub use dummy::Dummy;
/// Ethereum Client
pub use ethereum::Ethereum;

pub mod dummy;
pub mod ethereum;
