use async_trait::async_trait;
pub use dummy::Dummy;
pub use ethereum::Ethereum;
use primitives::adapter::client::{LockedClient, Unlockable, UnlockedClient};
use primitives::{
    adapter::{
        adapter2::{Error2, Locked, Unlocked},
        Deposit, Session,
    },
    Address, Channel, ValidatorId,
};
use std::{marker::PhantomData, sync::Arc};

pub mod dummy;
pub mod ethereum;

mod state {
    #[derive(Debug, Clone, Copy)]
    pub struct Locked;

    #[derive(Debug, Clone, Copy)]
    pub struct Unlocked;
}

// pub mod client {
//     use async_trait::async_trait;
//     use primitives::{
//         adapter::{Deposit, Session},
//         Address, Channel, ValidatorId,
//     };

//     #[async_trait]
//     /// Available methods for Locked clients.
//     pub trait LockedClient {
//         type Error: std::error::Error + Into<Error>;
//         /// Get Adapter whoami
//         fn whoami(&self) -> ValidatorId;

//         /// Verify, based on the signature & state_root, that the signer is the same
//         fn verify(
//             &self,
//             signer: ValidatorId,
//             state_root: &str,
//             signature: &str,
//         ) -> Result<bool, Self::Error>;

//         /// Creates a `Session` from a provided Token by calling the Contract.
//         /// Does **not** cache the (`Token`, `Session`) pair.
//         async fn session_from_token(&self, token: &str) -> Result<Session, Self::Error>;

//         async fn get_deposit(
//             &self,
//             channel: &Channel,
//             depositor_address: &Address,
//         ) -> Result<Deposit, Self::Error>;
//     }

//     /// Available methods for Unlocked clients.
//     /// Unlocked clients should also implement [`LockedClient`].
//     #[async_trait]
//     pub trait UnlockedClient: LockedClient {
//         // requires Unlocked
//         fn sign(&self, state_root: &str) -> Result<String, Self::Error>;

//         // requires Unlocked
//         async fn get_auth(&self, intended_for: ValidatorId) -> Result<String, Self::Error>;
//     }

//     /// A client that can be `unlock()`ed
//     /// and implements both [`LockedClient`] & [`UnlockedClient`].
//     ///
//     /// **Note:** A possibly expensive operation as it might result in cloning
//     pub trait Unlockable {
//         type Unlocked: UnlockedClient;

//         fn unlock(&self) -> Result<Self::Unlocked, <Self::Unlocked as LockedClient>::Error>;
//     }
// }

#[derive(Clone, Debug)]
pub struct Adapter<C, S = Locked> {
    /// client in a specific state - Locked or Unlocked
    client: Arc<C>,
    // /// We must use the `C` type from the definition
    _state: PhantomData<S>,
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

impl<C: LockedClient + Unlockable> Adapter<C, Locked>
where
    C::Error: Into<Error2>,
    <C::Unlocked as LockedClient>::Error: Into<Error2>,
{
    pub fn unlock(self) -> Result<Adapter<C::Unlocked, Unlocked>, Error2> {
        let unlocked = self.client.unlock().map_err(Into::into)?;

        Ok(Adapter {
            client: Arc::new(unlocked),
            _state: PhantomData::default(),
        })
    }
}

#[async_trait]
impl<C: UnlockedClient> UnlockedClient for Adapter<C, Unlocked>
where
    C: UnlockedClient + Send + Sync,
    C::Error: Into<Error2>,
{
    fn sign(&self, state_root: &str) -> Result<String, Error2> {
        Ok(state_root.to_string())
    }

    async fn get_auth(&self, intended_for: ValidatorId) -> Result<String, Error2> {
        Ok(intended_for.to_string())
    }
}

#[async_trait]
impl<C, S> LockedClient for Adapter<C, S>
where
    C: LockedClient + Send + Sync,
    C::Error: Into<Error2>,
    S: Sync + Send,
{
    type Error = Error2;

    fn whoami(&self) -> ValidatorId {
        todo!()
    }

    fn verify(
        &self,
        signer: ValidatorId,
        state_root: &str,
        signature: &str,
    ) -> Result<bool, Self::Error> {
        self.client
            .verify(signer, state_root, signature)
            .map_err(Into::into)
    }

    async fn session_from_token(&self, token: &str) -> Result<Session, Self::Error> {
        self.client
            .session_from_token(token)
            .await
            .map_err(Into::into)
    }

    async fn get_deposit(
        &self,
        channel: &Channel,
        depositor_address: Address,
    ) -> Result<Deposit, Error2> {
        self.client
            .get_deposit(channel, depositor_address)
            .await
            .map_err(Into::into)
    }
}

// #[derive(Debug)]
// /// The Errors that may occur when processing a `Request`.
// pub struct Error {
//     inner: Box<Inner>,
// }

// pub(crate) type BoxError = Box<dyn StdError + Send + Sync>;

// struct Inner {
//     kind: Kind,
//     source: Option<BoxError>,
// }

// impl fmt::Debug for Error {
//     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
//         let mut builder = f.debug_struct("reqwest::Error");

//         builder.field("kind", &self.inner.kind);

//         if let Some(ref url) = self.inner.url {
//             builder.field("url", url);
//         }
//         if let Some(ref source) = self.inner.source {
//             builder.field("source", source);
//         }

//         builder.finish()
//     }
// }

// impl Error {
//     pub(crate) fn new<E>(kind: Kind, source: Option<E>) -> Error
//     where
//         E: Into<BoxError>,
//     {
//         Error {
//             inner: Box::new(Inner {
//                 kind,
//                 source: source.map(Into::into),
//             }),
//         }
//     }
// }

// #[derive(Debug)]
// pub(crate) enum Kind {

// }
