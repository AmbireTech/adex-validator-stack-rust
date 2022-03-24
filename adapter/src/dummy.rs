//! The [`Dummy`] client for the [`Adapter`].
//!
use crate::{
    prelude::*,
    primitives::{Deposit, Session},
    Error,
};
use async_trait::async_trait;
use dashmap::{mapref::entry::Entry, DashMap};

use once_cell::sync::Lazy;
use primitives::{
    Address, Chain, ChainId, ChainOf, Channel, ChannelId, ToETHChecksum, ValidatorId,
};
use std::{collections::HashMap, sync::Arc};

pub type Adapter<S> = crate::Adapter<Dummy, S>;

/// The Dummy Chain to be used with this adapter
/// The Chain is not applicable to the adapter, however, it is required for
/// applications because of the `authentication` & [`Channel`] interactions.
pub static DUMMY_CHAIN: Lazy<Chain> = Lazy::new(|| Chain {
    chain_id: ChainId::new(1),
    rpc: "http://dummy.com".parse().expect("Should parse ApiUrl"),
    outpace: "0x0000000000000000000000000000000000000000"
        .parse()
        .unwrap(),
    sweeper: "0x0000000000000000000000000000000000000000"
        .parse()
        .unwrap(),
});

/// Dummy adapter implementation intended for testing.
#[derive(Debug, Clone)]
pub struct Dummy {
    /// Who am I
    identity: ValidatorId,
    /// Static authentication tokens (address => token)
    authorization_tokens: HashMap<Address, String>,
    deposits: Deposits,
}

pub struct Options {
    pub dummy_identity: ValidatorId,
    pub dummy_auth_tokens: HashMap<Address, String>,
}

#[derive(Debug, Clone, Default)]
#[allow(clippy::type_complexity)]
pub struct Deposits(Arc<DashMap<(ChannelId, Address), (usize, Vec<Deposit>)>>);

impl Deposits {
    pub fn add_deposit(&self, channel: ChannelId, address: Address, deposit: Deposit) {
        match self.0.entry((channel, address)) {
            Entry::Occupied(mut deposit_calls) => {
                // add the new deposit to the Vec
                deposit_calls.get_mut().1.push(deposit);
            }
            Entry::Vacant(empty) => {
                // add the new `(ChannelId, Address)` key and init with index 0 and the passed Deposit
                empty.insert((0, vec![deposit]));
            }
        }
    }

    pub fn get_next_deposit(&self, channel: ChannelId, address: Address) -> Option<Deposit> {
        match self.0.entry((channel, address)) {
            Entry::Occupied(mut entry) => {
                let (call_index, deposit_calls) = entry.get_mut();

                let deposit = deposit_calls.get(*call_index).cloned()?;

                // increment the index for the next call
                *call_index = call_index
                    .checked_add(1)
                    .expect("Deposit call index has overflowed");
                Some(deposit)
            }
            Entry::Vacant(_) => None,
        }
    }
}

impl Dummy {
    pub fn init(opts: Options) -> Self {
        Self {
            identity: opts.dummy_identity,
            authorization_tokens: opts.dummy_auth_tokens,
            deposits: Default::default(),
        }
    }

    pub fn add_deposit_call(&self, channel: ChannelId, address: Address, deposit: Deposit) {
        self.deposits.add_deposit(channel, address, deposit)
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
                chain: DUMMY_CHAIN.clone(),
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
        self.deposits
            .get_next_deposit(channel_context.context.id(), depositor_address)
            .ok_or_else(|| {
                Error::adapter(format!(
                    "No more mocked deposits found for depositor {:?}",
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

#[cfg(test)]
mod test {
    use std::num::NonZeroU8;

    use primitives::{
        config::TokenInfo,
        test_util::{CREATOR, DUMMY_CAMPAIGN, IDS, LEADER},
        BigNum, ChainOf, UnifiedNum,
    };

    use super::*;

    #[tokio::test]
    async fn test_deposits_calls() {
        let channel = DUMMY_CAMPAIGN.channel;

        let channel_context = ChainOf {
            context: channel,
            token: TokenInfo {
                min_token_units_for_deposit: 1_u64.into(),
                min_validator_fee: 1_u64.into(),
                precision: NonZeroU8::new(UnifiedNum::PRECISION).expect("Non zero u8"),
                address: channel.token,
            },
            chain: DUMMY_CHAIN.clone(),
        };

        let dummy_client = Dummy::init(Options {
            dummy_identity: IDS[&LEADER],
            dummy_auth_tokens: Default::default(),
        });

        let address = *CREATOR;

        // no mocked deposit calls should cause an Error
        {
            let result = dummy_client.get_deposit(&channel_context, address).await;

            assert!(result.is_err());
        }

        let get_deposit = |total: u64, create2: u64| Deposit {
            total: BigNum::from(total),
            still_on_create2: BigNum::from(create2),
        };

        // add two deposit and call 3 times
        // also check if different address does not have access to these calls
        {
            let deposits = [get_deposit(6969, 69), get_deposit(1000, 0)];
            dummy_client.add_deposit_call(channel.id(), address, deposits[0].clone());
            dummy_client.add_deposit_call(channel.id(), address, deposits[1].clone());

            let first_call = dummy_client
                .get_deposit(&channel_context, address)
                .await
                .expect("Should get first mocked deposit");
            assert_eq!(&deposits[0], &first_call);

            // should not affect the Mocked deposit calls and should cause an error
            let different_address_call = dummy_client.get_deposit(&channel_context, *LEADER).await;
            assert!(different_address_call.is_err());

            let second_call = dummy_client
                .get_deposit(&channel_context, address)
                .await
                .expect("Should get second mocked deposit");
            assert_eq!(&deposits[1], &second_call);

            // Third call should error, we've only mocked 2 calls!
            let third_call = dummy_client.get_deposit(&channel_context, address).await;
            assert!(third_call.is_err());
        }
    }
}
