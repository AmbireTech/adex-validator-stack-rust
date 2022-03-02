use std::num::NonZeroU8;

use adapter::{
    prelude::*,
    primitives::{Deposit, Session},
    Adapter, Error,
};
use async_trait::async_trait;

use primitives::{
    config::TokenInfo,
    test_util::{ADDRESS_1, DUMMY_CAMPAIGN},
    Address, BigNum, Chain, ChainId, ChainOf, Channel, ValidatorId,
};

#[derive(Debug, Clone)]
pub struct Dummy {
    _whoami: (),
}

#[async_trait]
impl Locked for Dummy {
    type Error = Error;
    /// Get Adapter whoami
    fn whoami(&self) -> ValidatorId {
        todo!()
    }

    /// Verify, based on the signature & state_root, that the signer is the same
    fn verify(
        &self,
        _signer: ValidatorId,
        _state_root: &str,
        _signature: &str,
    ) -> Result<bool, crate::Error> {
        todo!()
    }

    /// Creates a `Session` from a provided Token by calling the Contract.
    /// Does **not** cache the (`Token`, `Session`) pair.
    async fn session_from_token(&self, _token: &str) -> Result<Session, crate::Error> {
        todo!()
    }

    async fn get_deposit(
        &self,
        _channel_context: &ChainOf<Channel>,
        _depositor_address: Address,
    ) -> Result<Deposit, crate::Error> {
        Ok(Deposit {
            total: BigNum::from(42_u64),
            still_on_create2: BigNum::from(12_u64),
        })
    }
}

#[async_trait]
impl Unlocked for Dummy {
    // requires Unlocked
    fn sign(&self, state_root: &str) -> Result<String, Error> {
        Ok(state_root.to_string())
    }

    // requires Unlocked
    fn get_auth(&self, _for_chain: ChainId, intended_for: ValidatorId) -> Result<String, Error> {
        Ok(intended_for.to_string())
    }
}

impl Unlockable for Dummy {
    type Unlocked = Self;

    fn unlock(&self) -> Result<Self::Unlocked, Error> {
        Ok(self.clone())
    }
}

#[tokio::main]
async fn main() {
    let dummy = Dummy { _whoami: () };

    // A dummy Channel Context, with dummy Chain & Token
    let channel_context = ChainOf {
        context: DUMMY_CAMPAIGN.channel,
        token: TokenInfo {
            min_token_units_for_deposit: 1_u64.into(),
            min_validator_fee: 1_u64.into(),
            precision: NonZeroU8::new(18).unwrap(),
            address: "0x6B83e7D6B72c098d48968441e0d05658dc17Adb9"
                .parse()
                .unwrap(),
        },
        chain: Chain {
            chain_id: ChainId::new(1),
            rpc: "http://dummy.com".parse().unwrap(),
            outpace: "0x0000000000000000000000000000000000000000"
                .parse()
                .unwrap(),
            sweeper: "0x0000000000000000000000000000000000000000"
                .parse()
                .unwrap(),
        },
    };

    // With new Locked Adapter
    {
        let locked_adapter = Adapter::new(dummy.clone());

        // Should be able to call get_deposit before unlocking!
        locked_adapter
            .get_deposit(&channel_context, *ADDRESS_1)
            .await
            .expect("Should get deposit");

        // by default the Dummy adapter is Unlocked, but we still need to Unlock it!
        let unlocked_adapter = locked_adapter.unlock().expect("Should unlock");

        assert_eq!(
            "test".to_string(),
            unlocked_adapter.sign("test").expect("Ok")
        );

        // Should be able to call get_deposit after unlocking!
        unlocked_adapter
            .get_deposit(&channel_context, *ADDRESS_1)
            .await
            .expect("Should get deposit");
    }

    // with new Unlocked Adapter
    {
        let unlocked_adapter = Adapter::with_unlocked(dummy);

        // Should be able to call `get_deposit()` on unlocked adapter
        unlocked_adapter
            .get_deposit(&channel_context, *ADDRESS_1)
            .await
            .expect("Should get deposit");

        // Should be able to call `sign()` because adapter is already unlocked
        assert_eq!(
            "test".to_string(),
            unlocked_adapter.sign("test").expect("Ok")
        );
    }
}
