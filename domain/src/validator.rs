use serde::{Deserialize, Serialize};

pub use message::Message;

use crate::BigNum;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ValidatorDesc {
    pub id: String,
    pub url: String,
    pub fee: BigNum,
}

pub mod message {
    use chrono::{DateTime, Utc};
    use serde::{Deserialize, Serialize};

    use crate::BalancesMap;
    use serde::de::DeserializeOwned;
    use serde::export::fmt::Debug;

    pub trait State {
        type Signature: DeserializeOwned + Serialize + Debug;
        type StateRoot: DeserializeOwned + Serialize + Debug;
    }

    #[derive(Serialize, Deserialize, Debug)]
    #[serde(tag = "type")]
    pub enum Message<S: State> {
        #[serde(rename_all = "camelCase")]
        ApproveState(ApproveState<S>),
        #[serde(rename_all = "camelCase")]
        NewState(NewState<S>),
        #[serde(rename_all = "camelCase")]
        Heartbeat(Heartbeat<S>),
        #[serde(rename_all = "camelCase")]
        Accounting(Accounting),
    }

    #[derive(Serialize, Deserialize, Debug)]
    pub struct ApproveState<S: State> {
        state_root: S::StateRoot,
        signature: S::Signature,
        is_healthy: bool,
    }

    #[derive(Serialize, Deserialize, Debug)]
    pub struct NewState<S: State> {
        state_root: S::StateRoot,
        signature: S::Signature,
        balances: BalancesMap,
    }

    #[derive(Serialize, Deserialize, Debug)]
    pub struct Heartbeat<S: State> {
        signature: S::Signature,
        timestamp: DateTime<Utc>,
    }

    #[derive(Serialize, Deserialize, Debug)]
    pub struct Accounting {
        #[serde(rename = "last_ev_aggr")]
        last_event_aggregate: DateTime<Utc>,
        #[serde(rename = "balances_pre_fees")]
        pre_fees: BalancesMap,
        balances: BalancesMap,
    }

    #[cfg(any(test, feature = "fixtures"))]
    pub mod fixtures {
        use crate::test_util::time::past_datetime;

        use super::*;

        pub fn get_approved_state<S: State>() -> ApproveState<S> {
            unimplemented!()
        }

        pub fn get_new_state<S: State>() -> NewState<S> {
            unimplemented!()
        }

        pub fn get_heartbeat<S: State>() -> Heartbeat<S> {
            unimplemented!()
        }

        pub fn get_accounting() -> Accounting {
            Accounting {
                last_event_aggregate: past_datetime(None),
                pre_fees: Default::default(),
                balances: Default::default(),
            }
        }
    }
}

#[cfg(any(test, feature = "fixtures"))]
pub mod fixtures {
    use fake::faker::*;

    use crate::BigNum;

    use super::ValidatorDesc;

    pub fn get_validator(validator_id: &str, fee: Option<BigNum>) -> ValidatorDesc {
        let fee = fee.unwrap_or_else(|| BigNum::from(<Faker as Number>::between(1, 13)));
        let url = format!("http://{}-validator-url.com/validator", validator_id);

        ValidatorDesc {
            id: validator_id.to_string(),
            url,
            fee,
        }
    }

    pub fn get_validators(count: usize, prefix: Option<&str>) -> Vec<ValidatorDesc> {
        let prefix = prefix.map_or(String::new(), |prefix| format!("{}-", prefix));
        (0..count)
            .map(|c| {
                let validator_id = format!("{}validator-{}", prefix, c + 1);

                get_validator(&validator_id, None)
            })
            .collect()
    }
}
