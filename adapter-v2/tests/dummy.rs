use adapter_v3::{Adapter, Error, UnlockedClient, LockedClient, Unlockable};
use primitives::{ValidatorId, Channel, Address, adapter::Deposit, BigNum, test_util::{DUMMY_CAMPAIGN, ADDRESS_1}};

#[derive(Debug)]
pub struct Dummy {
    whoami: ()
}

impl Unlockable for Dummy {
    type Unlocked = Dummy;

    fn unlock(self) -> Result<Self::Unlocked, Error> {
        Ok(self)
    }
}

impl LockedClient for Dummy {
    fn get_deposit(
        &self,
        _channel: &Channel,
        _depositor_address: &Address,
    ) -> Result<Deposit, Error> {
        Ok(Deposit {
            total: BigNum::from(42_u64),
            still_on_create2: BigNum::from(12_u64),
        })
    }
}

impl UnlockedClient for Dummy {
    // requires Unlocked
    fn sign(&self, state_root: &str) -> Result<String, Error> {
        Ok(state_root.to_string())
    }

    // requires Unlocked
    fn get_auth(&self, intended_for: ValidatorId) -> Result<String, Error> {
        Ok(intended_for.to_string())
    }
}


fn main() {
    let dummy = Dummy { whoami: () };

    let adapter = Adapter::new(dummy);
    
    // Should be able to call get_deposit before unlocking!
    adapter.get_deposit(&DUMMY_CAMPAIGN.channel, &ADDRESS_1).expect("Should get deposit");
    
    // by default the Dummy adapter is Unlocked, but we still need to Unlock it!
    let unlocked = adapter.unlock().expect("Should unlock");

    assert_eq!("test".to_string(), unlocked.sign("test").expect("Ok"));

    // Should be able to call get_deposit after unlocking!
    unlocked.get_deposit(&DUMMY_CAMPAIGN.channel, &ADDRESS_1).expect("Should get deposit");
}
