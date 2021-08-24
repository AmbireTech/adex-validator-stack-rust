use std::{convert::TryFrom, marker::PhantomData};

use crate::{balances_map::UnifiedMap, channel_v5::Channel, Address, UnifiedNum};
use chrono::{DateTime, Utc};
use serde::{de::DeserializeOwned, Deserialize, Deserializer, Serialize};
use thiserror::Error;

#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Accounting<S: BalancesState> {
    pub channel: Channel,
    #[serde(flatten)]
    pub balances: Balances<S>,
    pub updated: Option<DateTime<Utc>>,
    pub created: DateTime<Utc>,
}

#[derive(Serialize, Debug, Clone, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct Balances<S: BalancesState> {
    pub earners: UnifiedMap,
    pub spenders: UnifiedMap,
    #[serde(skip_serializing, skip_deserializing)]
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
    fn validate(balances: Balances<UncheckedState>) -> Result<Balances<Self>, Error>;
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CheckedState;
impl BalancesState for CheckedState {
    fn validate(balances: Balances<UncheckedState>) -> Result<Balances<Self>, Error> {
        balances.check()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct UncheckedState;
impl BalancesState for UncheckedState {
    fn validate(balances: Balances<Self>) -> Result<Balances<Self>, Error> {
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
    use super::*;

    #[derive(Deserialize)]
    struct DeserializeAccounting {
        pub channel: Channel,
        #[serde(flatten)]
        pub balances: DeserializeBalances,
        pub created: DateTime<Utc>,
        pub updated: Option<DateTime<Utc>>,
    }

    impl<'de> Deserialize<'de> for Accounting<UncheckedState> {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            let de_acc = DeserializeAccounting::deserialize(deserializer)?;

            Ok(Self {
                channel: de_acc.channel,
                balances: Balances::<UncheckedState>::try_from(de_acc.balances)
                    .map_err(serde::de::Error::custom)?,
                created: de_acc.created,
                updated: de_acc.updated,
            })
        }
    }

    impl<'de> Deserialize<'de> for Accounting<CheckedState> {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            let unchecked_acc = Accounting::<UncheckedState>::deserialize(deserializer)?;

            Ok(Self {
                channel: unchecked_acc.channel,
                balances: unchecked_acc
                    .balances
                    .check()
                    .map_err(serde::de::Error::custom)?,
                created: unchecked_acc.created,
                updated: unchecked_acc.updated,
            })
        }
    }

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

            S::validate(unchecked_balances).map_err(serde::de::Error::custom)
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
