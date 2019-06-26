use std::convert::TryFrom;

use serde::{Deserialize, Serialize};

pub use message::Message;

use crate::{BigNum, DomainError};

pub mod message;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(transparent)]
pub struct ValidatorId(String);

impl TryFrom<&str> for ValidatorId {
    type Error = DomainError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        // @TODO: Should we have some constrains(like valid hex string starting with `0x`)? If not this should be just `From`.
        Ok(Self(value.to_string()))
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ValidatorDesc {
    // @TODO: Replace id `String` with `ValidatorId` https://github.com/AdExNetwork/adex-validator-stack-rust/issues/83
    pub id: String,
    pub url: String,
    pub fee: BigNum,
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
