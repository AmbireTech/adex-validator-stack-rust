use std::{marker::PhantomData, ops::Deref};
use primitives::{ValidatorId, Address, Channel, adapter::Deposit, BigNum};

#[derive(Debug)]
pub struct Locked<T: LockedClient>(T);
#[derive(Debug)]
pub struct Unlocked<T: UnlockedClient>(T);

mod impls;

pub struct Adapter<C, S = Locked<C>> {
    /// client in a specific state - Locked or Unlocked
    client: S,
    /// We must use the `C` type from the definition
    _phantom: PhantomData<C>,
}

impl<C: LockedClient> Adapter<C> {
    pub fn new(client: C) -> Self {
        Self {
            client: Locked(client),
            _phantom: PhantomData::default(),
        }
    }
}

impl<C: UnlockedClient> Adapter<C> {
    pub fn with_unlocked(client: C) -> Adapter<C, Unlocked<C>> {
        Adapter {
            client: Unlocked(client),
            _phantom: PhantomData::default(),
        }
    }
}

#[derive(Debug)]
pub struct Error {}

impl<C: LockedClient + Unlockable> Adapter<C, Locked<C>> {
    pub fn unlock(self) -> Result<Adapter<C::Unlocked, Unlocked<C::Unlocked>>, Error> {
        let unlocked = self.client.0.unlock()?;

        Ok(Adapter {
            client: Unlocked(unlocked),
            _phantom: PhantomData::default()
        })
    }
}

impl<C: UnlockedClient> Adapter<C, Unlocked<C>> {
    pub fn sign(&self, state_root: &str) -> Result<String, Error> {
        Ok(state_root.to_string())
    }

    pub fn get_auth(&self, intended_for: ValidatorId) -> Result<String, Error> {
        Ok(intended_for.to_string())
    }
}

impl<C: LockedClient, S: Deref<Target=C>> Adapter<C, S> {
    pub fn get_deposit(
        &self,
        channel: &Channel,
        depositor_address: &Address,
    ) -> Result<Deposit, Error> {
        self.client.deref().get_deposit(channel, depositor_address)
    }
}

pub trait LockedClient {
    fn get_deposit(
        &self,
        channel: &Channel,
        depositor_address: &Address,
    ) -> Result<Deposit, Error>;
}
pub trait UnlockedClient: LockedClient {
    // requires Unlocked
    fn sign(&self, state_root: &str) -> Result<String, Error>;

    // requires Unlocked
    fn get_auth(&self, intended_for: ValidatorId) -> Result<String, Error>;
}

// pub trait Client: fmt::Debug + Unlockable {
//     // requires Unlocked
//     fn sign(&self, state_root: &str) -> Result<String, Error>;

//     // requires Unlocked
//     fn get_auth(&self, intended_for: ValidatorId) -> Result<String, Error>;

//     fn get_deposit(
//         &self,
//         channel: &Channel,
//         depositor_address: &Address,
//     ) -> Result<Deposit, Error>;
// }

pub trait Unlockable {
    type Unlocked: UnlockedClient;

    fn unlock(self) -> Result<Self::Unlocked, Error>;
}

#[derive(Debug)]
pub struct UnlockedWallet {
    wallet: (),
    password: (),
}

#[derive(Debug)]
pub struct LockedWallet {
    keystore: (),
    password: (),
}

pub trait WalletState {}
impl WalletState for UnlockedWallet {}
impl WalletState for LockedWallet {}


#[derive(Debug)]
pub struct Ethereum<S = LockedWallet> {
    web3: (),
    keystore: (),
    keystore_pwd: (),
    state: S,
}

impl Unlockable for Ethereum<LockedWallet> {
    type Unlocked = Ethereum<UnlockedWallet>;

    fn unlock(self) -> Result<Ethereum<UnlockedWallet>, Error> {
        Ok(Ethereum {
            web3: self.web3,
            keystore: self.keystore,
            keystore_pwd: self.keystore_pwd.clone(),
            state: UnlockedWallet {
                wallet: (),
                password: self.keystore_pwd.clone(),
            }
        })
    }
}

impl Unlockable for Ethereum<UnlockedWallet> {
    type Unlocked = Self;

    fn unlock(self) -> Result<Self, Error> {
        Ok(self)
    }
}

impl<S: WalletState> LockedClient for Ethereum<S> {
    fn get_deposit(
        &self,
        channel: &Channel,
        depositor_address: &Address,
    ) -> Result<Deposit, Error> {
        Ok(Deposit {
            total: BigNum::from(42_u64),
            still_on_create2: BigNum::from(12_u64),
        })
    }
}

impl UnlockedClient for Ethereum<UnlockedWallet> {
    fn sign(&self, state_root: &str) -> Result<String, Error> {
        Ok(state_root.to_string())
    }

    fn get_auth(&self, intended_for: ValidatorId) -> Result<String, Error> {
        Ok(intended_for.to_string())
    }
}

#[cfg(test)]
mod test {
    use primitives::test_util::{ADDRESS_1, DUMMY_CAMPAIGN};

    use super::*;

    #[test]
    fn use_adapter() {
        // With Locked Client
        {
            let ethereum = Ethereum {
                web3: (),
                keystore: (),
                keystore_pwd: (),
                state: LockedWallet {
                    keystore: (),
                    password: (),
                },
                // state: UnlockedWallet { wallet: (), password: () },
            };
            let adapter = Adapter::new(ethereum);

            // Should be able to call get_deposit before unlocking!
            adapter.get_deposit(&DUMMY_CAMPAIGN.channel, &ADDRESS_1).expect("Should get deposit");
    
            let unlocked = adapter.unlock().expect("Should unlock");

            unlocked.get_auth((*ADDRESS_1).into()).expect("Should get Auth");

        }
        
        // with Unlocked Client
        {
            let ethereum = Ethereum {
                web3: (),
                keystore: (),
                keystore_pwd: (),
                state: UnlockedWallet { wallet: (), password: () },
            };

            let adapter = Adapter::with_unlocked(ethereum);
            
            adapter.get_deposit(&DUMMY_CAMPAIGN.channel, &ADDRESS_1).expect("Should get deposit");
            adapter.get_auth((*ADDRESS_1).into()).expect("Should get Auth");
        }
    }
}