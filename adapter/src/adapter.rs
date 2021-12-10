use crate::primitives::*;
use async_trait::async_trait;
use std::{marker::PhantomData, sync::Arc};

use crate::{prelude::*, Error};

pub use self::state::{Locked, Unlocked};

pub type UnlockedC = Adapter<dyn UnlockedClient<Error = crate::Error>, Unlocked>;
pub type LockedC = Adapter<dyn LockedClient<Error = crate::Error>, Unlocked>;

mod state {
    #[derive(Debug, Clone, Copy)]
    /// The `Locked` state of the [`crate::Adapter`].
    /// See [`crate::client::LockedClient`]
    pub struct Locked;

    /// The `Unlocked` state of the [`crate::Adapter`].
    /// See [`crate::client::UnlockedClient`]
    #[derive(Debug, Clone, Copy)]
    pub struct Unlocked;
}

#[derive(Debug)]
/// [`Adapter`] struct
pub struct Adapter<C, S = Locked> {
    /// client in a specific state - Locked or Unlocked
    pub client: Arc<C>,
    // /// We must use the `C` type from the definition
    _state: PhantomData<S>,
}

impl<C, S: Clone> Clone for Adapter<C, S> {
    fn clone(&self) -> Self {
        Self {
            client: self.client.clone(),
            _state: self._state.clone(),
        }
    }
}

impl<C: LockedClient> Adapter<C> {
    /// Create a new [`Adapter`] in [`Locked`] state using a [`LockedClient`].
    pub fn new(client: C) -> Adapter<C, Locked> {
        Adapter {
            client: Arc::new(client),
            _state: PhantomData::default(),
        }
    }
}

impl<C: UnlockedClient> Adapter<C, Unlocked> {
    /// Create a new [`Adapter`] in [`Unlocked`] state using an [`UnlockedClient`].
    pub fn with_unlocked(client: C) -> Adapter<C, Unlocked> {
        Adapter {
            client: Arc::new(client),
            _state: PhantomData::default(),
        }
    }
}

impl<C> Adapter<C, Locked>
where
    C: LockedClient + Unlockable,
    <C::Unlocked as LockedClient>::Error: Into<Error>,
    C::Error: Into<Error>,
{
    pub fn unlock(self) -> Result<Adapter<C::Unlocked, Unlocked>, Error> {
        let unlocked = self.client.unlock().map_err(Into::into)?;

        Ok(Adapter {
            client: Arc::new(unlocked),
            _state: PhantomData::default(),
        })
    }
}

#[async_trait]
impl<C> UnlockedClient for Adapter<C, Unlocked>
where
    C: UnlockedClient + Sync + Send,
    C::Error: Into<Error>,
{
    fn sign(&self, state_root: &str) -> Result<String, Error> {
        Ok(state_root.to_string())
    }

    async fn get_auth(&self, intended_for: ValidatorId) -> Result<String, Error> {
        self.client.get_auth(intended_for).await.map_err(Into::into)
    }
}

#[async_trait]
impl<C, S> LockedClient for Adapter<C, S>
where
    C: LockedClient + Sync + Send,
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
    /// Does **not** cache the (`Token`, `Session`) pair.
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
