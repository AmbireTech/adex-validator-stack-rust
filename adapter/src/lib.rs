#![deny(rust_2018_idioms)]
#![deny(clippy::all)]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![allow(deprecated)]

pub use {
    self::adapter::{
        state::{LockedState, UnlockedState},
        Adapter,
    },
    dummy::Dummy,
    error::Error,
    ethereum::Ethereum,
};

/// Primitives used by the [`Adapter`].
/// Including re-exported types from the `primitives` crate that are being used.
pub mod primitives {
    use serde::{Deserialize, Serialize};

    pub use ::primitives::{
        config::{ChainInfo, Config, TokenInfo},
        Address, BigNum, Chain, ChainId, ChainOf, Channel, ValidatorId,
    };

    use crate::ethereum::WalletState;

    /// The [`Deposit`] struct with [`BigNum`] values.
    /// Returned by [`crate::client::Locked::get_deposit`]
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

    /// [`Session`] struct returned by the [`crate::Adapter`] when [`crate::client::Locked::session_from_token`] is called.
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Session {
        pub era: i64,
        /// Authenticated as [`Address`].
        pub uid: Address,
        /// Authenticated for [`Chain`].
        pub chain: Chain,
    }
}

/// Re-export of the [`crate::client`] traits and the states of the [`Adapter`].
pub mod prelude {
    /// Re-export traits used for working with the [`crate::Adapter`].
    pub use crate::client::{Locked, Unlockable, Unlocked};

    pub use crate::{LockedState, UnlockedState};
}

mod adapter;

pub mod client;

pub mod dummy;

mod error;

pub mod ethereum;
pub mod util;
