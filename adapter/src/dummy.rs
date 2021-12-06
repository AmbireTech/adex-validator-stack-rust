use async_trait::async_trait;
use dashmap::{mapref::entry::Entry, DashMap};
use primitives::{
    adapter::{
        Adapter, AdapterErrorKind, AdapterResult, Deposit, DummyAdapterOptions,
        Error as AdapterError, Session,
    },
    config::Config,
    Address, Channel, ChannelId, ToETHChecksum, ValidatorId,
};
use std::{collections::HashMap, fmt, sync::Arc};

#[derive(Debug, Clone)]
pub struct DummyAdapter {
    /// Who am I
    identity: ValidatorId,
    config: Config,
    /// Auth tokens that we have verified (tokenId => session)
    session_tokens: HashMap<String, Address>,
    /// Auth tokens that we've generated to authenticate with someone (address => token)
    authorization_tokens: HashMap<String, String>,
    deposits: Deposits,
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

impl DummyAdapter {
    pub fn init(opts: DummyAdapterOptions, config: &Config) -> Self {
        Self {
            identity: opts.dummy_identity,
            config: config.to_owned(),
            session_tokens: opts.dummy_auth,
            authorization_tokens: opts.dummy_auth_tokens,
            deposits: Default::default(),
        }
    }

    pub fn add_deposit_call(&self, channel: ChannelId, address: Address, deposit: Deposit) {
        self.deposits.add_deposit(channel, address, deposit)
    }
}

#[derive(Debug)]
pub struct Error {}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Dummy Adapter error occurred!")
    }
}

impl AdapterErrorKind for Error {}

#[async_trait]
impl Adapter for DummyAdapter {
    type AdapterError = Error;

    fn unlock(&mut self) -> AdapterResult<(), Self::AdapterError> {
        Ok(())
    }

    fn whoami(&self) -> ValidatorId {
        self.identity
    }

    fn sign(&self, state_root: &str) -> AdapterResult<String, Self::AdapterError> {
        let signature = format!(
            "Dummy adapter signature for {} by {}",
            state_root,
            self.whoami().to_checksum()
        );
        Ok(signature)
    }

    fn verify(
        &self,
        signer: ValidatorId,
        _state_root: &str,
        signature: &str,
    ) -> AdapterResult<bool, Self::AdapterError> {
        // select the `identity` and compare it to the signer
        // for empty string this will return array with 1 element - an empty string `[""]`
        let is_same = match signature.rsplit(' ').take(1).next() {
            Some(from) => from == signer.to_checksum(),
            None => false,
        };

        Ok(is_same)
    }

    async fn session_from_token<'a>(
        &'a self,
        token: &'a str,
    ) -> AdapterResult<Session, Self::AdapterError> {
        let identity = self
            .authorization_tokens
            .iter()
            .find(|(_, id)| *id == token);

        match identity {
            Some((id, _)) => Ok(Session {
                uid: self.session_tokens[id],
                era: 0,
            }),
            None => Err(AdapterError::Authentication(format!(
                "no session token for this auth: {}",
                token
            ))),
        }
    }

    fn get_auth(&self, _validator: &ValidatorId) -> AdapterResult<String, Self::AdapterError> {
        let who = self
            .session_tokens
            .iter()
            .find(|(_, id)| *id == &self.identity.to_address());
        match who {
            Some((id, _)) => {
                let auth = self.authorization_tokens.get(id).expect("id should exist");
                Ok(auth.to_owned())
            }
            None => Err(AdapterError::Authentication(format!(
                "no auth token for this identity: {}",
                self.identity
            ))),
        }
    }

    async fn get_deposit(
        &self,
        channel: &Channel,
        address: &Address,
    ) -> AdapterResult<Deposit, Self::AdapterError> {
        self.deposits
            .get_next_deposit(channel.id(), *address)
            .ok_or_else(|| AdapterError::Adapter(Box::new(Error {})))
    }
}

#[cfg(test)]
mod test {
    use primitives::{
        config::DEVELOPMENT_CONFIG,
        util::tests::prep_db::{ADDRESSES, DUMMY_CAMPAIGN, IDS},
        BigNum,
    };

    use super::*;

    #[tokio::test]
    async fn test_deposits_calls() {
        let config = DEVELOPMENT_CONFIG.clone();
        let channel = DUMMY_CAMPAIGN.channel;
        let adapter = DummyAdapter::init(
            DummyAdapterOptions {
                dummy_identity: IDS["leader"],
                dummy_auth: Default::default(),
                dummy_auth_tokens: Default::default(),
            },
            &config,
        );

        let address = ADDRESSES["creator"];

        // no mocked deposit calls should cause an Error
        {
            let result = adapter.get_deposit(&channel, &address).await;

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
            adapter.add_deposit_call(channel.id(), address, deposits[0].clone());
            adapter.add_deposit_call(channel.id(), address, deposits[1].clone());

            let first_call = adapter
                .get_deposit(&channel, &address)
                .await
                .expect("Should get first mocked deposit");
            assert_eq!(&deposits[0], &first_call);

            // should not affect the Mocked deposit calls and should cause an error
            let different_address_call = adapter.get_deposit(&channel, &ADDRESSES["leader"]).await;
            assert!(different_address_call.is_err());

            let second_call = adapter
                .get_deposit(&channel, &address)
                .await
                .expect("Should get second mocked deposit");
            assert_eq!(&deposits[1], &second_call);

            // Third call should error, we've only mocked 2 calls!
            let third_call = adapter.get_deposit(&channel, &address).await;
            assert!(third_call.is_err());
        }
    }
}
