pub use {
    self::adapter::{
        state::{LockedState, UnlockedState},
        Adapter,
    },
    error::Error,
};

pub use dummy::Dummy;
pub use ethereum::Ethereum;

/// only re-export types from the `primitives` crate that are being used by the [`crate::Adapter`].
pub mod primitives {
    use serde::{Deserialize, Serialize};

    /// Re-export all the types used from the [`primitives`] crate.
    pub use ::primitives::{Address, BigNum, Channel, ValidatorId};

    use crate::ethereum::WalletState;

    pub type Deposit = ::primitives::Deposit<primitives::BigNum>;

    /// A helper type that allows you to use either of them
    /// and dereference the adapter when calling for example an application
    /// with a concrete implementation of the [`crate::Adapter`].
    pub enum AdapterTypes<S, ES> {
        Dummy(Box<crate::dummy::Adapter<S>>),
        Ethereum(Box<crate::Adapter<crate::Ethereum<ES>, S>>),
    }

    impl<S, ES: WalletState> AdapterTypes<S, ES> {
        pub fn dummy(dummy: crate::dummy::Adapter<S>) -> Self {
            Self::Dummy(Box::new(dummy))
        }

        pub fn ethereum(ethereum: crate::Adapter<crate::Ethereum<ES>, S>) -> Self {
            Self::Ethereum(Box::new(ethereum))
        }
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Session {
        pub era: i64,
        pub uid: Address,
    }
}

/// Re-export of the [`crate::client`] traits.
pub mod prelude {
    /// Re-export traits used for working with the [`crate::Adapter`].
    pub use crate::client::{Locked, Unlockable, Unlocked};

    pub use crate::{LockedState, UnlockedState};
}

/// The [`Adapter`] struct and it's states
/// Used for communication with underlying implementation.
/// 2 Adapters are available in this crate:
/// - [`crate::ethereum::Adapter`] and it's client implementation [`crate::Ethereum`] for chains compatible with EVM.
/// - [`crate::dummy::Adapter`] and it's client implementation [`crate::Dummy`] for testing.
mod adapter;
pub mod client;
pub mod dummy;
mod error;
pub mod ethereum;
pub mod util;
