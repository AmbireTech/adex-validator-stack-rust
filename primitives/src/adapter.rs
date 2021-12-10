use crate::{Address, BigNum, Channel, ValidatorId};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, convert::From, fmt};

pub type AdapterResult<T, AE> = Result<T, Error<AE>>;

pub trait AdapterErrorKind: fmt::Debug + fmt::Display {}
pub type Deposit = crate::Deposit<BigNum>;

mod state {
    #[derive(Debug, Clone, Copy)]
    pub struct Locked;

    #[derive(Debug, Clone, Copy)]
    pub struct Unlocked;
}

pub mod client {
    use crate::{
        adapter::{Deposit, Session},
        Address, Channel, ValidatorId,
    };
    use async_trait::async_trait;

    #[async_trait]
    /// Available methods for Locked clients.
    pub trait LockedClient {
        type Error: std::error::Error;
        /// Get Adapter whoami
        fn whoami(&self) -> ValidatorId;

        /// Verify, based on the signature & state_root, that the signer is the same
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
            channel: &Channel,
            depositor_address: Address,
        ) -> Result<Deposit, Self::Error>;

        // fn unlock(
        //     &self,
        // ) -> Result<
        //     <Self as Unlockable>::Unlocked,
        //     <<Self as Unlockable>::Unlocked as LockedClient>::Error,
        // >
        // where
        //     Self: Unlockable,
        // {
        //     <Self as Unlockable>::unlock(self)
        // }
    }

    /// Available methods for Unlocked clients.
    /// Unlocked clients should also implement [`LockedClient`].
    #[async_trait]
    pub trait UnlockedClient: LockedClient {
        // requires Unlocked
        fn sign(&self, state_root: &str) -> Result<String, Self::Error>;

        // requires Unlocked
        async fn get_auth(&self, intended_for: ValidatorId) -> Result<String, Self::Error>;
    }

    /// A client that can be `unlock()`ed
    /// and implements both [`LockedClient`] & [`UnlockedClient`].
    ///
    /// **Note:** A possibly expensive operation as it might result in cloning
    pub trait Unlockable {
        type Unlocked: UnlockedClient;

        fn unlock(&self) -> Result<Self::Unlocked, <Self::Unlocked as LockedClient>::Error>;
    }
}

pub mod adapter2 {
    use super::{
        client::{LockedClient, Unlockable, UnlockedClient},
        Session,
    };
    use crate::{adapter::Deposit, Address, Channel, ValidatorId};
    use async_trait::async_trait;
    use parse_display::Display;
    use std::{error::Error as StdError, fmt};
    use std::{marker::PhantomData, sync::Arc};
    use thiserror::Error;

    pub use super::state::{Locked, Unlocked};

    pub(crate) type BoxError = Box<dyn StdError + Send + Sync>;

    #[derive(Debug, Error)]
    #[error("{inner}")]
    pub struct Error2 {
        inner: Box<Inner>,
    }

    impl Error2 {
        pub(crate) fn new<E>(kind: Kind, source: Option<E>) -> Self
        where
            E: Into<BoxError>,
        {
            Self {
                inner: Box::new(Inner {
                    kind,
                    source: source.map(Into::into),
                }),
            }
        }

        pub fn wallet_unlock<E>(source: E) -> Self
        where
            E: Into<BoxError>,
        {
            Self::new(Kind::WalletUnlock, Some(source))
        }

        pub fn authentication<E>(source: E) -> Self
        where
            E: Into<BoxError>,
        {
            Self::new(Kind::Authentication, Some(source))
        }

        pub fn authorization<E>(source: E) -> Self
        where
            E: Into<BoxError>,
        {
            Self::new(Kind::Authorization, Some(source))
        }

        pub fn adapter<A>(source: A) -> Self
        where
            A: Into<BoxError>,
        {
            Self::new(Kind::Adapter, Some(source))
        }

        pub fn verify<A>(source: A) -> Self
        where
            A: Into<BoxError>,
        {
            Self::new(Kind::Verify, Some(source))
        }
    }
    #[derive(Debug, Error)]
    struct Inner {
        kind: Kind,
        source: Option<BoxError>,
    }

    impl fmt::Display for Inner {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match &self.source {
                // Writes: "Kind: Error message here"
                Some(source) => write!(f, "{}: {}", self.kind, source.to_string()),
                // Writes: "Kind"
                None => write!(f, "{}", self.kind),
            }
        }
    }

    #[derive(Debug, Display)]
    pub(crate) enum Kind {
        Adapter,
        WalletUnlock,
        Verify,
        Authentication,
        Authorization,
    }

    // impl<E: Into<BoxError>> From<E> for Error2 {
    //     fn from(error: E) -> Self {
    //     Error2::adapter(error)
    // }
    // }

    // pub trait AdapterError: StdError {}

    // impl AdapterError for Box<dyn StdError + Send + 'static> {}

    // we want to be able to create an error from an adapter specific error
    // impl<A: AdapterError> Into<Box<dyn StdError + Send>> for A {
    //     fn into(self) -> Self {
    //         Self::new(Kind::Adapter, Some(self))
    //     }
    // }

    // impl<A: AdapterError + Send + Sync> From<A> for Error2 {
    //     fn from(adapter_error: A) -> Self {
    //         Self::new(Kind::Adapter, Some(adapter_error))
    //     }
    // }

    // impl<A: AdapterError + Send + Sync> From<A> for Error2 {
    //     fn from(adapter_error: A) -> Self {
    //         Self {
    //             inner: Box::new(Inner {
    //                 kind: Kind::Adapter,
    //                 source: Some(Box::new(adapter_error)),
    //             }),
    //         }
    //     }
    // }

    #[derive(Clone, Debug)]
    pub struct Adapter<C, S = Locked> {
        /// client in a specific state - Locked or Unlocked
        pub client: Arc<C>,
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

    impl<C> Adapter<C, Locked>
    where
        C: LockedClient + Unlockable,
        <C::Unlocked as LockedClient>::Error: Into<Error2>,
        C::Error: Into<Error2>,
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
    impl<C> UnlockedClient for Adapter<C, Unlocked>
    where
        C: UnlockedClient + Sync + Send,
        C::Error: Into<Error2>,
    {
        fn sign(&self, state_root: &str) -> Result<String, Error2> {
            Ok(state_root.to_string())
        }

        async fn get_auth(&self, intended_for: ValidatorId) -> Result<String, Error2> {
            self.client.get_auth(intended_for).await.map_err(Into::into)
        }
    }

    #[async_trait]
    impl<C, S> LockedClient for Adapter<C, S>
    where
        C: LockedClient + Sync + Send,
        C::Error: Into<Error2>,
        S: Sync + Send,
    {
        type Error = Error2;
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
        ) -> Result<bool, Error2> {
            self.client
                .verify(signer, state_root, signature)
                .map_err(Into::into)
        }

        /// Creates a `Session` from a provided Token by calling the Contract.
        /// Does **not** cache the (`Token`, `Session`) pair.
        async fn session_from_token(&self, token: &str) -> Result<Session, Error2> {
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
}

#[derive(Debug)]
pub enum Error<AE: AdapterErrorKind> {
    Authentication(String),
    Authorization(String),
    /// Adapter specific errors
    // Since we don't know the size of the Adapter Error we use a Box to limit the size of this enum
    Adapter(Box<AE>),
    /// You need to `.unlock()` the wallet first
    LockedWallet,
}

impl<AE: AdapterErrorKind> std::error::Error for Error<AE> {}

impl<AE: AdapterErrorKind> From<AE> for Error<AE> {
    fn from(adapter_err: AE) -> Self {
        Self::Adapter(Box::new(adapter_err))
    }
}

impl<AE: AdapterErrorKind> fmt::Display for Error<AE> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Authentication(error) => write!(f, "Authentication: {}", error),
            Error::Authorization(error) => write!(f, "Authorization: {}", error),
            Error::Adapter(error) => write!(f, "Adapter: {}", *error),
            Error::LockedWallet => write!(f, "You must `.unlock()` the wallet first"),
        }
    }
}

pub struct DummyAdapterOptions {
    pub dummy_identity: ValidatorId,
    pub dummy_auth: HashMap<String, Address>,
    pub dummy_auth_tokens: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub struct KeystoreOptions {
    pub keystore_file: String,
    pub keystore_pwd: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub era: i64,
    pub uid: Address,
}

#[async_trait]
pub trait Adapter: Send + Sync + fmt::Debug + Clone {
    type AdapterError: AdapterErrorKind + 'static;

    /// Unlock adapter
    fn unlock(&mut self) -> AdapterResult<(), Self::AdapterError>;

    /// Get Adapter whoami
    fn whoami(&self) -> ValidatorId;

    /// Signs the provided state_root
    fn sign(&self, state_root: &str) -> AdapterResult<String, Self::AdapterError>;

    /// Verify, based on the signature & state_root, that the signer is the same
    fn verify(
        &self,
        signer: ValidatorId,
        state_root: &str,
        signature: &str,
    ) -> AdapterResult<bool, Self::AdapterError>;

    /// Get user session from token
    async fn session_from_token<'a>(
        &'a self,
        token: &'a str,
    ) -> AdapterResult<Session, Self::AdapterError>;

    /// Gets authentication for specific validator
    fn get_auth(&self, validator_id: &ValidatorId) -> AdapterResult<String, Self::AdapterError>;

    /// Calculates and returns the total spendable amount
    async fn get_deposit(
        &self,
        channel: &Channel,
        address: &Address,
    ) -> AdapterResult<Deposit, Self::AdapterError>;
}
