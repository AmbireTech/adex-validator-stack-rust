use crate::primitives::*;
use async_trait::async_trait;
use std::{marker::PhantomData, sync::Arc};

use crate::{
    client::{Locked, Unlockable, Unlocked},
    Error,
};

// pub use self::state::{Locked, Unlocked};

// pub type UnlockedC = Adapter<dyn Unlocked<Error = crate::Error>, state::UnlockedState>;
// pub type LockedC = Adapter<dyn Locked<Error = crate::Error>, state::UnlockedState>;

pub(crate) mod state {
    #[derive(Debug, Clone, Copy)]
    /// The `Locked` state of the [`crate::Adapter`].
    /// See [`crate::client::Locked`]
    pub struct LockedState;

    /// The `Unlocked` state of the [`crate::Adapter`].
    /// See [`crate::client::Unlocked`]
    #[derive(Debug, Clone, Copy)]
    pub struct UnlockedState;
}

#[derive(Debug)]
/// The [`Adapter`] struct and it's states.
///
/// Used for communication with the underlying client implementation.
///
/// # Available adapters
///
/// 2 Adapters are available in this crate:
/// - Ethereum
///   - [`crate::ethereum::LockedAdapter`] and [`crate::ethereum::UnlockedAdapter`]
///   - Client implementation [`crate::Ethereum`] for chains compatible with EVM.
/// - Dummy
///   - [`crate::dummy::Adapter`] and it's client implementation [`crate::Dummy`] for testing.
pub struct Adapter<C, S = state::LockedState> {
    /// client in a specific state - Locked or Unlocked
    pub client: Arc<C>,
    // /// We must use the `C` type from the definition
    _state: PhantomData<S>,
}

impl<C, S: Clone> Clone for Adapter<C, S> {
    fn clone(&self) -> Self {
        Self {
            client: self.client.clone(),
            _state: self._state,
        }
    }
}

impl<C: Locked> Adapter<C> {
    /// Create a new [`Adapter`] in [`Locked`] state using a [`Locked`].
    pub fn new(client: C) -> Adapter<C, state::LockedState> {
        Adapter {
            client: Arc::new(client),
            _state: PhantomData::default(),
        }
    }
}

impl<C: Unlocked> Adapter<C, state::LockedState> {
    /// Create a new [`Adapter`] in [`state::UnlockedState`] state using an [`Unlocked`] client.
    pub fn with_unlocked(client: C) -> Adapter<C, state::UnlockedState> {
        Adapter {
            client: Arc::new(client),
            _state: PhantomData::default(),
        }
    }
}

impl<C> Adapter<C, state::LockedState>
where
    C: Locked + Unlockable,
    <C::Unlocked as Locked>::Error: Into<Error>,
    C::Error: Into<Error>,
{
    pub fn unlock(self) -> Result<Adapter<C::Unlocked, state::UnlockedState>, Error> {
        let unlocked = self.client.unlock().map_err(Into::into)?;

        Ok(Adapter {
            client: Arc::new(unlocked),
            _state: PhantomData::default(),
        })
    }
}

#[async_trait]
impl<C> Unlocked for Adapter<C, state::UnlockedState>
where
    C: Unlocked + Sync + Send,
    C::Error: Into<Error>,
{
    fn sign(&self, state_root: &str) -> Result<String, Error> {
        self.client.sign(state_root).map_err(Into::into)
    }

    fn get_auth(&self, intended_for: ValidatorId) -> Result<String, Error> {
        self.client.get_auth(intended_for).map_err(Into::into)
    }
}

#[async_trait]
impl<C, S> Locked for Adapter<C, S>
where
    C: Locked + Sync + Send,
    C::Error: Into<Error>,
    S: Sync + Send,
{
    type Error = Error;
    /// Get Adapter whoami
    fn whoami(&self) -> ValidatorId {
        self.client.whoami()
    }

    /// Verify, based on the signature & state_root, that the signer is the same
    fn verify(
        &self,
        signer: ValidatorId,
        state_root: &str,
        signature: &str,
    ) -> Result<bool, Error> {
        self.client
            .verify(signer, state_root, signature)
            .map_err(Into::into)
    }

    /// Creates a `Session` from a provided Token by calling the Contract.
    /// Does **not** cache the (`Token`, [`Session`]) pair.
    async fn session_from_token(&self, token: &str) -> Result<Session, Error> {
        self.client
            .session_from_token(token)
            .await
            .map_err(Into::into)
    }

    async fn get_deposit(
        &self,
        channel: &Channel,
        depositor_address: Address,
    ) -> Result<Deposit, Error> {
        self.client
            .get_deposit(channel, depositor_address)
            .await
            .map_err(Into::into)
    }
}
