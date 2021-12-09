use adapter_v2::{
    Adapter,
};
use async_trait::async_trait;
use primitives::{
    adapter::{Deposit, adapter2::Error2, client::{LockedClient, Unlockable, UnlockedClient}, Session},
    test_util::{ADDRESS_1, DUMMY_CAMPAIGN},
    Address, BigNum, Channel, ValidatorId,
};

#[derive(Debug, Clone)]
pub struct Dummy {
    whoami: (),
}

#[async_trait]
impl LockedClient for Dummy {
    type Error = Error2;
    /// Get Adapter whoami
    fn whoami(&self) -> ValidatorId {
        todo!()
    }

    /// Verify, based on the signature & state_root, that the signer is the same
    fn verify(
        &self,
        signer: ValidatorId,
        state_root: &str,
        signature: &str,
    ) -> Result<bool, Self::Error> {
        todo!()
    }

    /// Creates a `Session` from a provided Token by calling the Contract.
    /// Does **not** cache the (`Token`, `Session`) pair.
    async fn session_from_token(&self, token: &str) -> Result<Session, Self::Error> {
        todo!()
    }

    async fn get_deposit(
        &self,
        _channel: &Channel,
        _depositor_address: &Address,
    ) -> Result<Deposit, Self::Error> {
        Ok(Deposit {
            total: BigNum::from(42_u64),
            still_on_create2: BigNum::from(12_u64),
        })
    }
}

#[async_trait]
impl UnlockedClient for Dummy {
    // requires Unlocked
    fn sign(&self, state_root: &str) -> Result<String, Error2> {
        Ok(state_root.to_string())
    }

    // requires Unlocked
    async fn get_auth(&self, intended_for: ValidatorId) -> Result<String, Error2> {
        Ok(intended_for.to_string())
    }
}

impl Unlockable for Dummy {
    type Unlocked = Self;

    fn unlock(&self) -> Result<Self::Unlocked, Error2> {
        Ok(self.clone())
    }
}

#[tokio::main]
async fn main() {
    let dummy = Dummy { whoami: () };

    // With new Locked Adapter
    {
        let locked_adapter = Adapter::new(dummy.clone());

        // Should be able to call get_deposit before unlocking!
        locked_adapter
            .get_deposit(&DUMMY_CAMPAIGN.channel, &ADDRESS_1)
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
            .get_deposit(&DUMMY_CAMPAIGN.channel, &ADDRESS_1)
            .await
            .expect("Should get deposit");
    }

    // with new Unlocked Adapter
    {
        let unlocked_adapter = Adapter::with_unlocked(dummy);

        // Should be able to call `get_deposit()` on unlocked adapter
        unlocked_adapter
            .get_deposit(&DUMMY_CAMPAIGN.channel, &ADDRESS_1)
            .await
            .expect("Should get deposit");

        // Should be able to call `sign()` because adapter is already unlocked
        assert_eq!(
            "test".to_string(),
            unlocked_adapter.sign("test").expect("Ok")
        );
    }
}
