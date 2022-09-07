use std::marker::PhantomData;

use crate::{Address, UnifiedMap, UnifiedNum};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use thiserror::Error;

#[derive(Serialize, Debug, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Balances<S: BalancesState = CheckedState> {
    pub earners: UnifiedMap,
    pub spenders: UnifiedMap,
    #[serde(skip)]
    state: PhantomData<S>,
}

impl Balances<UncheckedState> {
    pub fn check(self) -> Result<Balances<CheckedState>, Error> {
        let earned = self
            .earners
            .values()
            .sum::<Option<UnifiedNum>>()
            .ok_or_else(|| Error::Overflow("earners overflow".to_string()))?;
        let spent = self
            .spenders
            .values()
            .sum::<Option<UnifiedNum>>()
            .ok_or_else(|| Error::Overflow("spenders overflow".to_string()))?;

        if earned != spent {
            Err(Error::PayoutMismatch { spent, earned })
        } else {
            Ok(Balances {
                earners: self.earners,
                spenders: self.spenders,
                state: PhantomData::<CheckedState>::default(),
            })
        }
    }
}

impl<S: BalancesState> Balances<S> {
    pub fn new() -> Balances<S> {
        Balances {
            earners: Default::default(),
            spenders: Default::default(),
            state: Default::default(),
        }
    }

    /// Adds the spender to the Balances with `0` if he does not exist
    pub fn add_spender(&mut self, spender: Address) {
        self.spenders
            .entry(spender)
            .or_insert_with(UnifiedNum::default);
    }

    /// Adds the earner to the Balances with `0` if he does not exist
    pub fn add_earner(&mut self, earner: Address) {
        self.earners
            .entry(earner)
            .or_insert_with(UnifiedNum::default);
    }

    pub fn into_unchecked(self) -> Balances<UncheckedState> {
        Balances {
            earners: self.earners,
            spenders: self.spenders,
            state: PhantomData::default(),
        }
    }

    /// Returns a tuple of the sum of `(earners, spenders)`
    pub fn sum(&self) -> Option<(UnifiedNum, UnifiedNum)> {
        self.spenders
            .values()
            .sum::<Option<UnifiedNum>>()
            .and_then(|spenders| {
                let earners = self.earners.values().sum::<Option<UnifiedNum>>()?;

                Some((earners, spenders))
            })
    }
}

impl Balances<CheckedState> {
    pub fn spend(
        &mut self,
        spender: Address,
        earner: Address,
        amount: UnifiedNum,
    ) -> Result<(), OverflowError> {
        let spent = self.spenders.entry(spender).or_default();
        *spent = spent
            .checked_add(&amount)
            .ok_or(OverflowError::Spender(spender))?;

        let earned = self.earners.entry(earner).or_default();
        *earned = earned
            .checked_add(&amount)
            .ok_or(OverflowError::Earner(earner))?;

        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum OverflowError {
    #[error("Spender {0} amount overflowed")]
    Spender(Address),
    #[error("Earner {0} amount overflowed")]
    Earner(Address),
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("Overflow of computation {0}")]
    Overflow(String),
    #[error("Payout mismatch between spent ({spent}) and earned ({earned})")]
    PayoutMismatch {
        spent: UnifiedNum,
        earned: UnifiedNum,
    },
}

pub trait BalancesState: std::fmt::Debug + Eq + Clone + Serialize + DeserializeOwned {
    fn from_unchecked(balances: Balances<UncheckedState>) -> Result<Balances<Self>, Error>;
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CheckedState;
impl BalancesState for CheckedState {
    fn from_unchecked(balances: Balances<UncheckedState>) -> Result<Balances<Self>, Error> {
        balances.check()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct UncheckedState;
impl BalancesState for UncheckedState {
    fn from_unchecked(balances: Balances<Self>) -> Result<Balances<Self>, Error> {
        Ok(balances)
    }
}

impl TryFrom<Balances<UncheckedState>> for Balances<CheckedState> {
    type Error = Error;

    fn try_from(value: Balances<UncheckedState>) -> Result<Self, Self::Error> {
        value.check()
    }
}

/// This modules implements the needed non-generic structs that help with Deserialization of the `Balances<S>`
mod de {
    use serde::Deserializer;

    use super::*;

    #[derive(Deserialize, Debug, Clone, PartialEq, Eq)]
    struct DeserializeBalances {
        pub earners: UnifiedMap,
        pub spenders: UnifiedMap,
    }

    impl<'de, S: BalancesState> Deserialize<'de> for Balances<S> {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            let deser_balances = DeserializeBalances::deserialize(deserializer)?;

            let unchecked_balances = Balances {
                earners: deser_balances.earners,
                spenders: deser_balances.spenders,
                state: PhantomData::<UncheckedState>::default(),
            };

            S::from_unchecked(unchecked_balances).map_err(serde::de::Error::custom)
        }
    }

    impl From<DeserializeBalances> for Balances<UncheckedState> {
        fn from(value: DeserializeBalances) -> Self {
            Self {
                earners: value.earners,
                spenders: value.spenders,
                state: PhantomData::<UncheckedState>::default(),
            }
        }
    }
}
