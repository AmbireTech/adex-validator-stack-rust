use std::str::FromStr;

use num_bigint::BigUint;
use serde::{Serialize, Deserialize, Deserializer, Serializer};

#[derive(Serialize, Deserialize, Debug)]
pub struct BigNum(
    #[serde(deserialize_with = "biguint_from_str", serialize_with = "biguint_to_str")]
    BigUint
);

fn biguint_from_str<'de, D>(deserializer: D) -> Result<BigUint, D::Error>
    where
        D: Deserializer<'de>,
{
    let num = String::deserialize(deserializer)?;
    Ok(BigUint::from_str(&num).map_err(serde::de::Error::custom)?)
}

fn biguint_to_str<S>(num: &BigUint, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer
{
    serializer.serialize_str(&num.to_str_radix(10))
}