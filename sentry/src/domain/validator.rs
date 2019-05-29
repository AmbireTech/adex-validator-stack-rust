use serde::{Deserialize, Serialize};

use crate::domain::BigNum;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ValidatorDesc {
    pub addr: String,
    pub url: String,
    pub fee: BigNum,
}

#[cfg(test)]
pub(crate) mod fixtures {
    use std::convert::TryFrom;

    use fake::faker::*;

    use crate::domain::BigNum;

    use super::ValidatorDesc;

    pub fn get_validator(validator_id: &str) -> ValidatorDesc {
        let fee = BigNum::try_from(<Faker as Number>::between(1_u32, 13_u32)).expect("BigNum error when creating from random number");
        let url = format!("http://{}-validator-url.com/validator", validator_id);

        ValidatorDesc {
            addr: validator_id.to_string(),
            url,
            fee,
        }
    }

    pub fn get_validators(count: usize, prefix: Option<&str>) -> Vec<ValidatorDesc> {
        let prefix = prefix.map_or(String::new(), |prefix| format!("{}-", prefix));
        (0..count)
            .map(|c| {
                let validator_id = format!("{}validator-{}", prefix, c + 1);

                get_validator(&validator_id)
            })
            .collect()
    }
}