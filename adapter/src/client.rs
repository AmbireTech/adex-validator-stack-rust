//! The Client traits that define the actions for each state.
//!
//! - [`Locked`]
//! - [`Unlocked`]
//! - [`Unlockable`]

use crate::primitives::{Deposit, Session};
use async_trait::async_trait;
use primitives::{Address, ChainId, ChainOf, Channel, ValidatorId};

#[async_trait]
/// Available methods for Locked clients.
pub trait Locked: Sync + Send {
    type Error: std::error::Error + Into<crate::Error> + 'static;

    /// Get Adapter whoami
    fn whoami(&self) -> ValidatorId;

    /// Verify, based on the `signature` & `state_root`, that the `signer` is the same.
    ///
    /// `signature` should be a `0x` prefixed hex string.
    ///
    /// `state_root` should be a hex string, with no `0x` prefix.
    fn verify(
        &self,
        signer: ValidatorId,
        state_root: &str,
        signature: &str,
    ) -> Result<bool, Self::Error>;

    /// Creates a `Session` from a provided Token by calling the Contract.
    /// Does **not** cache the (`Token`, `Session`) pair.
    async fn session_from_token(&self, token: &str) -> Result<Session, Self::Error>;

    async fn get_deposit(
        &self,
        channel_context: &ChainOf<Channel>,
        depositor_address: Address,
    ) -> Result<Deposit, Self::Error>;

    // fn unlock(
    //     &self,
    // ) -> Result<
    //     <Self as Unlockable>::Unlocked,
    //     <<Self as Unlockable>::Unlocked as Locked>::Error,
    // >
    // where
    //     Self: Unlockable,
    // {
    //     <Self as Unlockable>::unlock(self)
    // }
}

/// Available methods for Unlocked clients.
///
/// Unlocked clients should also implement [`Locked`].
#[async_trait]
pub trait Unlocked: Locked {
    // requires Unlocked
    fn sign(&self, state_root: &str) -> Result<String, Self::Error>;

    // requires Unlocked
    fn get_auth(
        &self,
        for_chain: ChainId,
        intended_for: ValidatorId,
    ) -> Result<String, Self::Error>;
}

/// A client that can be `unlock()`ed
/// and implements both [`Locked`] & [`Unlocked`].
///
/// **Note:** A possibly expensive operation as it might result in cloning
pub trait Unlockable {
    type Unlocked: Unlocked;

    fn unlock(&self) -> Result<Self::Unlocked, <Self::Unlocked as Locked>::Error>;
}
