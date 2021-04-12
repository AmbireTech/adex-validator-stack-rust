use async_trait::async_trait;
use lazy_static::lazy_static;
use primitives::{
    adapter::{
        Adapter, AdapterErrorKind, AdapterResult, Deposit, DummyAdapterOptions,
        Error as AdapterError, Session,
    },
    channel_v5::{Channel as ChannelV5, Nonce},
    channel_validator::ChannelValidator,
    config::{Config, TokenInfo},
    Address, BigNum, Channel, ChannelId, ToETHChecksum, ValidatorId,
};
use std::ops::Add;
use std::{collections::HashMap, convert::TryFrom, fmt};

#[derive(Debug, Clone)]
pub struct DummyAdapter {
    identity: ValidatorId,
    config: Config,
    // Auth tokens that we have verified (tokenId => session)
    session_tokens: HashMap<String, ValidatorId>,
    // Auth tokens that we've generated to authenticate with someone (address => token)
    authorization_tokens: HashMap<String, String>,
    // Generated for retrieving deposit of channel-spender pair
    deposits: HashMap<(ChannelId, Address), BigNum>,
    // Generated for retrieving balances of tokens
    token_balances: HashMap<Address, BigNum>,
}

lazy_static! {
    static ref DUMMY_V5_CHANNEL: ChannelV5 = ChannelV5 {
        leader: ValidatorId::try_from("2bdeafae53940669daa6f519373f686c1f3d3393")
            .expect("failed to create id"),
        follower: ValidatorId::try_from("6704Fbfcd5Ef766B287262fA2281C105d57246a6")
            .expect("failed to create id"),
        guardian: Address::try_from("0000000000000000000000000000000000000000")
            .expect("failed to create address"),
        token: Address::try_from("0x73967c6a0904aa032c103b4104747e88c566b1a2")
            .expect("should create an address"),
        nonce: Nonce::from(12345_u32),
    };
}

// Enables DummyAdapter to be able to
// check if a channel is valid
impl ChannelValidator for DummyAdapter {}

impl DummyAdapter {
    pub fn init(opts: DummyAdapterOptions, config: &Config) -> Self {
        Self {
            identity: opts.dummy_identity,
            config: config.to_owned(),
            session_tokens: opts.dummy_auth,
            authorization_tokens: opts.dummy_auth_tokens,
            deposits: generate_deposits(),
            token_balances: generate_balances(&config.token_address_whitelist),
        }
    }
}

fn generate_deposits() -> HashMap<(ChannelId, Address), BigNum> {
    let mut deposits = HashMap::new();
    deposits.insert(
        (
            DUMMY_V5_CHANNEL.id(),
            Address::try_from("1111111111111111111111111111111111111111").expect("should generate"),
        ),
        BigNum::try_from("1000000000000").expect("should make bignum"),
    );
    deposits
}

fn generate_balances(whitelist: &HashMap<Address, TokenInfo>) -> HashMap<Address, BigNum> {
    let mut balances = HashMap::new();
    whitelist.keys().for_each(|k| {
        balances.insert(
            *k,
            whitelist
                .get(k)
                .unwrap()
                .min_token_units_for_deposit
                .clone(),
        );
    });
    balances
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

    fn whoami(&self) -> &ValidatorId {
        &self.identity
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
        signer: &ValidatorId,
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

    async fn validate_channel<'a>(
        &'a self,
        channel: &'a Channel,
    ) -> AdapterResult<bool, Self::AdapterError> {
        DummyAdapter::is_channel_valid(&self.config, self.whoami(), channel)
            .map(|_| true)
            .map_err(AdapterError::InvalidChannel)
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
            .find(|(_, id)| *id == &self.identity);
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
        channel: &ChannelV5,
        address: &Address,
    ) -> AdapterResult<Deposit, Self::AdapterError> {
        let mut total = self
            .deposits
            .get(&(channel.id(), *address))
            .unwrap()
            .clone();
        let still_on_create_2 = self.token_balances.get(&channel.token).unwrap().clone();
        let token_info = self
            .config
            .token_address_whitelist
            .get(&channel.token)
            .unwrap();

        if still_on_create_2 > token_info.min_token_units_for_deposit {
            total = total.add(still_on_create_2.clone());
        }

        Ok(Deposit {
            total,
            still_on_create_2,
        })
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use primitives::{
        config::configuration,
        util::tests::prep_db::{AUTH, IDS},
    };
    use std::convert::TryFrom;

    fn setup_dummy_adapter(dummy_identity: &str) -> DummyAdapter {
        let config = configuration("development", None).expect("failed parse config");

        let options = DummyAdapterOptions {
            dummy_identity: ValidatorId::try_from(dummy_identity).expect("should generate id"),
            dummy_auth: IDS.clone(),
            dummy_auth_tokens: AUTH.clone(),
        };

        DummyAdapter::init(options, &config)
    }

    #[tokio::test]
    async fn test_dummy_get_deposit() {
        let dummy_adapter = setup_dummy_adapter("0000000000000000000000000000000000000000");

        let spender: Address = Address::try_from("1111111111111111111111111111111111111111")
            .expect("should create an address");
        let deposit = dummy_adapter
            .get_deposit(&DUMMY_V5_CHANNEL, &spender)
            .await
            .expect("should get deposit");
        let expected_total = dummy_adapter
            .deposits
            .get(&(DUMMY_V5_CHANNEL.id(), spender))
            .expect("should get balance");
        assert_eq!(deposit.total, *expected_total);
        let expected_on_create_2 = dummy_adapter
            .token_balances
            .get(&DUMMY_V5_CHANNEL.token)
            .expect("should get token");
        assert_eq!(deposit.still_on_create_2, *expected_on_create_2);
    }
}
