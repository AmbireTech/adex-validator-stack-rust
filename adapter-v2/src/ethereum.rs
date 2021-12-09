use async_trait::async_trait;
use primitives::{
    adapter::{
        adapter2::Error2,
        client::{LockedClient, Unlockable, UnlockedClient},
        Deposit, Session,
    },
    Address, BigNum, Channel, ValidatorId,
};
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct UnlockedWallet {
    wallet: (),
    password: (),
}

#[derive(Debug, Clone)]
pub enum LockedWallet {
    KeyStore { keystore: (), password: () },
    PrivateKey(String),
}

pub trait WalletState: Send + Sync {}
impl WalletState for UnlockedWallet {}
impl WalletState for LockedWallet {}

#[derive(Debug, Clone)]
pub struct Ethereum<S = LockedWallet> {
    web3: (),
    state: S,
}

#[derive(Debug, Error)]
#[error("Error!")]
pub enum EthereumError {}
impl Into<Error2> for EthereumError {
    fn into(self) -> Error2 {
        Error2::adapter(self)
    }
}

impl Unlockable for Ethereum<LockedWallet> {
    type Unlocked = Ethereum<UnlockedWallet>;

    fn unlock(&self) -> Result<Ethereum<UnlockedWallet>, EthereumError> {
        let unlocked_wallet = match &self.state {
            LockedWallet::KeyStore { keystore, password } => UnlockedWallet {
                wallet: (),
                password: password.clone(),
            },
            LockedWallet::PrivateKey(_priv_key) => todo!(),
        };

        Ok(Ethereum {
            web3: self.web3.clone(),
            state: unlocked_wallet,
        })
    }
}

#[async_trait]
impl<S: WalletState> LockedClient for Ethereum<S> {
    type Error = EthereumError;
    async fn get_deposit(
        &self,
        _channel: &Channel,
        _depositor_address: &Address,
    ) -> Result<Deposit, EthereumError> {
        Ok(Deposit {
            total: BigNum::from(42_u64),
            still_on_create2: BigNum::from(12_u64),
        })
    }

    fn whoami(&self) -> ValidatorId {
        todo!()
    }

    fn verify(
        &self,
        signer: ValidatorId,
        state_root: &str,
        signature: &str,
    ) -> Result<bool, Self::Error> {
        todo!()
    }

    async fn session_from_token(&self, token: &str) -> Result<Session, Self::Error> {
        todo!()
    }
}

#[async_trait]
impl UnlockedClient for Ethereum<UnlockedWallet> {
    fn sign(&self, state_root: &str) -> Result<String, EthereumError> {
        Ok(state_root.to_string())
    }

    async fn get_auth(&self, intended_for: ValidatorId) -> Result<String, EthereumError> {
        Ok(intended_for.to_string())
    }
}

#[cfg(test)]
mod test {
    use primitives::{
        adapter::adapter2::Adapter,
        test_util::{ADDRESS_1, DUMMY_CAMPAIGN},
    };

    use super::*;

    #[tokio::test]
    async fn use_adapter() {
        // With Locked Client
        {
            let ethereum = Ethereum {
                web3: (),
                state: LockedWallet::KeyStore {
                    keystore: (),
                    password: (),
                },
            };
            let adapter = Adapter::new(ethereum);

            // Should be able to call get_deposit before unlocking!
            adapter
                .get_deposit(&DUMMY_CAMPAIGN.channel, &ADDRESS_1)
                .await
                .expect("Should get deposit");

            let unlocked = adapter.unlock().expect("Should unlock");

            unlocked
                .get_auth((*ADDRESS_1).into())
                .await
                .expect("Should get Auth");
        }

        // with Unlocked Client
        {
            let ethereum = Ethereum {
                web3: (),
                state: UnlockedWallet {
                    wallet: (),
                    password: (),
                },
            };

            let adapter = Adapter::with_unlocked(ethereum);

            adapter
                .get_deposit(&DUMMY_CAMPAIGN.channel, &ADDRESS_1)
                .await
                .expect("Should get deposit");
            adapter
                .get_auth((*ADDRESS_1).into())
                .await
                .expect("Should get Auth");
        }
    }
}
