//! The [`Dummy`] client for the [`Adapter`].
//!
use crate::{
    prelude::*,
    primitives::{Deposit, Session},
    Error,
};
use async_trait::async_trait;

use primitives::{
    config::ChainInfo, Address, ChainId, ChainOf, Channel, ToETHChecksum, ValidatorId,
};
use std::collections::HashMap;

#[doc(inline)]
pub use self::deposit::Deposits;

pub type Adapter<S> = crate::Adapter<Dummy, S>;

#[cfg(feature = "test-util")]
#[cfg_attr(docsrs, doc(cfg(feature = "test-util")))]
pub mod test_util {
    use once_cell::sync::Lazy;
    use primitives::{
        config::{ChainInfo, TokenInfo, DUMMY_CONFIG},
        Chain,
    };
    pub static DUMMY_TOKEN: Lazy<TokenInfo> =
        Lazy::new(|| DUMMY_CONFIG.chains["Dummy"].tokens["DUMMY"].clone());

    pub static DUMMY_CHAIN_INFO: Lazy<ChainInfo> =
        Lazy::new(|| DUMMY_CONFIG.chains["Dummy"].clone());

    /// The Dummy Chain to be used with this adapter
    /// The Chain is not applicable to the adapter, however, it is required for
    /// applications because of the `authentication` & [`Channel`] interactions.
    pub static DUMMY_CHAIN: Lazy<Chain> = Lazy::new(|| DUMMY_CHAIN_INFO.chain.clone());
}

#[derive(Debug, Clone)]
pub struct Options {
    /// The identity used for the Adapter.
    pub dummy_identity: ValidatorId,
    /// The authentication tokens that will be used by the adapter
    /// for returning & validating authentication tokens of requests.
    pub dummy_auth_tokens: HashMap<Address, String>,
    /// The [`ChainInfo`] that will be used for the [`Session`]s and
    /// also for the deposits.
    pub dummy_chain: ChainInfo,
}

/// Dummy adapter implementation intended for testing.
#[derive(Debug, Clone)]
pub struct Dummy {
    /// Who am I
    identity: ValidatorId,
    /// Static authentication tokens (address => token)
    authorization_tokens: HashMap<Address, String>,
    chain_info: ChainInfo,
    deposits: Deposits,
}

impl Dummy {
    pub fn init(opts: Options) -> Self {
        Self {
            identity: opts.dummy_identity,
            authorization_tokens: opts.dummy_auth_tokens,
            chain_info: opts.dummy_chain,
            deposits: Default::default(),
        }
    }

    /// Set the deposit that you want the adapter to return every time
    /// when the [`get_deposit()`](Locked::get_deposit) get's called
    /// for the give [`ChannelId`] and [`Address`].
    ///
    /// If [`Deposit`] is set to [`None`], it remove the mocked deposit.
    ///
    /// # Panics
    ///
    /// When [`None`] is passed but there was no mocked deposit.
    pub fn set_deposit<D: Into<Option<Deposit>>>(
        &self,
        channel_context: &ChainOf<Channel>,
        depositor: Address,
        deposit: D,
    ) {
        use deposit::Key;

        let key = Key::from_chain_of(channel_context, depositor);
        match deposit.into() {
            Some(deposit) => {
                self.deposits.0.insert(key, deposit);
            }
            None => {
                self.deposits.0.remove(&key).unwrap_or_else(|| {
                    panic!("Couldn't remove a deposit which doesn't exist for {key:?}")
                });
            }
        };
    }
}

#[async_trait]
impl Locked for Dummy {
    type Error = Error;
    /// Get Adapter whoami
    fn whoami(&self) -> ValidatorId {
        self.identity
    }

    /// Verify, based on the signature & state_root, that the signer is the same
    ///
    /// Splits the signature by `" "` (whitespace) and takes
    /// the last part of it which contains the signer [`Address`].
    fn verify(
        &self,
        signer: ValidatorId,
        _state_root: &str,
        signature: &str,
    ) -> Result<bool, crate::Error> {
        // select the `identity` and compare it to the signer
        // for empty string this will return array with 1 element - an empty string `[""]`
        let is_same = match signature.rsplit(' ').take(1).next() {
            Some(from) => from == signer.to_checksum(),
            None => false,
        };

        Ok(is_same)
    }

    /// Finds the authorization token from the configured values
    /// and creates a [`Session`] out of it using a [`ChainId`] of `1`.
    async fn session_from_token(&self, token: &str) -> Result<Session, crate::Error> {
        let identity = self
            .authorization_tokens
            .iter()
            .find(|(_, address_token)| *address_token == token);

        match identity {
            Some((address, _token)) => Ok(Session {
                uid: *address,
                era: 0,
                chain: self.chain_info.chain.clone(),
            }),
            None => Err(Error::authentication(format!(
                "No identity found that matches authentication token: {}",
                token
            ))),
        }
    }

    async fn get_deposit(
        &self,
        channel_context: &ChainOf<Channel>,
        depositor_address: Address,
    ) -> Result<Deposit, crate::Error> {
        // validate that the same chain & token are used for the Channel Context
        // as the ones setup in the Dummy adapter.
        if channel_context.token.address != channel_context.context.token {
            return Err(Error::adapter(
                "Token context of channel & channel token addresses are different".to_string(),
            ));
        }

        if self.chain_info.chain != channel_context.chain
            || self
                .chain_info
                .find_token(channel_context.context.token)
                .is_none()
        {
            return Err(Error::adapter(
                "Channel's Token & Chain not aligned with Dummy adapter's chain".to_string(),
            ));
        }

        self.deposits
            .get_deposit(channel_context, depositor_address)
            .ok_or_else(|| {
                Error::adapter(format!(
                    "No mocked deposit found for {:?} & depositor {:?}",
                    channel_context.context.id(),
                    depositor_address
                ))
            })
    }
}

#[async_trait]
impl Unlocked for Dummy {
    // requires Unlocked
    fn sign(&self, state_root: &str) -> Result<String, Error> {
        let signature = format!(
            "Dummy adapter signature for {} by {}",
            state_root,
            self.whoami().to_checksum()
        );
        Ok(signature)
    }

    // requires Unlocked
    fn get_auth(&self, _for_chain: ChainId, _intended_for: ValidatorId) -> Result<String, Error> {
        self.authorization_tokens
            .get(&self.identity.to_address())
            .cloned()
            .ok_or_else(|| {
                Error::authentication(format!(
                    "No auth token for this identity: {}",
                    self.identity
                ))
            })
    }
}

impl Unlockable for Dummy {
    type Unlocked = Self;

    fn unlock(&self) -> Result<Self::Unlocked, Error> {
        Ok(self.clone())
    }
}

mod deposit {
    use crate::primitives::Deposit;
    use dashmap::DashMap;
    use primitives::{Address, ChainId, ChainOf, Channel, ChannelId};
    use std::sync::Arc;

    #[derive(Debug, Clone, Copy, Hash, Eq, PartialEq)]
    pub struct Key {
        channel_id: ChannelId,
        chain_id: ChainId,
        depositor: Address,
    }

    impl Key {
        pub fn from_chain_of(channel_context: &ChainOf<Channel>, depositor: Address) -> Self {
            Self {
                channel_id: channel_context.context.id(),
                chain_id: channel_context.chain.chain_id,
                depositor,
            }
        }
    }

    /// Mocked deposits for the Dummy adapter.
    ///
    /// These deposits can be set once and the adapter will return
    /// the set deposit on every call to [`get_deposit()`](`Locked::get_deposit`).
    #[derive(Debug, Clone, Default)]
    pub struct Deposits(pub Arc<DashMap<Key, Deposit>>);

    impl Deposits {
        pub fn new() -> Self {
            Self::default()
        }

        /// Get's the set deposit for the give [`ChannelId`] and [`Address`].
        ///
        /// This method will return [`None`] if the deposit for the pair
        /// [`ChannelId`] & [`Address`] was not set.
        pub fn get_deposit(
            &self,
            channel: &ChainOf<Channel>,
            depositor: Address,
        ) -> Option<Deposit> {
            self.0
                .get(&Key::from_chain_of(channel, depositor))
                .map(|dashmap_ref| dashmap_ref.value().clone())
        }
    }
}

#[cfg(test)]
mod test {
    use primitives::{
        test_util::{CREATOR, DUMMY_CAMPAIGN, IDS, LEADER, PUBLISHER},
        BigNum, ChainOf,
    };

    use super::{
        test_util::{DUMMY_CHAIN, DUMMY_CHAIN_INFO, DUMMY_TOKEN},
        *,
    };

    #[tokio::test]
    async fn test_deposits_calls() {
        let channel = Channel {
            token: DUMMY_TOKEN.address,
            ..DUMMY_CAMPAIGN.channel
        };

        let channel_context = ChainOf {
            context: channel,
            token: DUMMY_TOKEN.clone(),
            chain: DUMMY_CHAIN.clone(),
        };

        let dummy_client = Dummy::init(Options {
            dummy_identity: IDS[&LEADER],
            dummy_auth_tokens: Default::default(),
            dummy_chain: DUMMY_CHAIN_INFO.clone(),
        });

        let creator = *CREATOR;
        let publisher = *PUBLISHER;

        // no mocked deposit calls should cause an Error
        {
            let result = dummy_client.get_deposit(&channel_context, creator).await;

            assert!(result.is_err());
        }

        let get_deposit = |total: u64| Deposit {
            total: BigNum::from(total),
        };

        // add two deposit for CREATOR & PUBLISHER
        {
            let creator_deposit = get_deposit(6969);
            let publisher_deposit = get_deposit(1000);

            dummy_client.set_deposit(&channel_context, creator, creator_deposit.clone());
            dummy_client.set_deposit(&channel_context, publisher, publisher_deposit.clone());

            let creator_actual = dummy_client
                .get_deposit(&channel_context, creator)
                .await
                .expect("Should get mocked deposit");
            assert_eq!(&creator_deposit, &creator_actual);

            // calling an non-mocked address, should cause an error
            let different_address_call = dummy_client.get_deposit(&channel_context, *LEADER).await;
            assert!(different_address_call.is_err());

            let publisher_actual = dummy_client
                .get_deposit(&channel_context, publisher)
                .await
                .expect("Should get mocked deposit");
            assert_eq!(&publisher_deposit, &publisher_actual);
        }
    }

    #[test]
    #[should_panic]
    fn test_set_deposit_to_none_should_panic_on_non_mocked_deposits() {
        let channel = DUMMY_CAMPAIGN.channel;

        let channel_context = ChainOf {
            context: channel,
            token: DUMMY_TOKEN.clone(),
            chain: DUMMY_CHAIN.clone(),
        };

        let dummy_client = Dummy::init(Options {
            dummy_identity: IDS[&LEADER],
            dummy_auth_tokens: Default::default(),
            dummy_chain: DUMMY_CHAIN_INFO.clone(),
        });

        // It should panic when no deposit is set and we try to set it to None
        dummy_client.set_deposit(&channel_context, *LEADER, None);
    }
}
