use serde::{Deserialize, Serialize};

pub use message::{Message, State};

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

    pub trait State {
        type Signature;
        type StateRoot;
    }

    #[derive(Serialize, Deserialize, Debug)]
    #[serde(tag = "type")]
    pub enum Message<S: State> {
        #[serde(rename_all = "camelCase")]
        ApproveState {
            state_root: S::StateRoot,
            signature: S::Signature,
            is_healthy: bool,
        },
        #[serde(rename_all = "camelCase")]
        NewState {
            state_root: S::StateRoot,
            signature: S::Signature,
            balances: BalancesMap,
        },
        #[serde(rename_all = "camelCase")]
        Heartbeat {
            signature: S::Signature,
            timestamp: DateTime<Utc>,
        },
        #[serde(rename_all = "camelCase")]
        Accounting {
            last_ev_aggr: DateTime<Utc>,
            balances_pre_fees: BalancesMap,
            balances: BalancesMap,
        },
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
