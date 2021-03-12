use serde::{Deserialize, Serialize, Serializer};
use std::fmt;

use crate::{ToHex, targeting::Value, DomainError, ToETHChecksum};
use std::convert::TryFrom;

#[derive(Deserialize, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(transparent)]
pub struct Address(
    #[serde(
        deserialize_with = "ser::from_str",
        serialize_with = "SerHex::<StrictPfx>::serialize"
    )]
    [u8; 20],
);

impl Address {
    pub fn as_bytes(&self) -> &[u8; 20] {
        &self.0
    }
}

impl Serialize for Address {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let checksum = self.to_checksum();
        serializer.serialize_str(&checksum)
    }
}

impl fmt::Debug for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Address({})", self.to_hex_prefixed())
    }
}

impl ToETHChecksum for Address {}

impl From<&[u8; 20]> for Address {
    fn from(bytes: &[u8; 20]) -> Self {
        Self(*bytes)
    }
}

impl AsRef<[u8]> for Address {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl TryFrom<&str> for Address {
    type Error = DomainError;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let hex_value = match value {
            value if value.len() == 42 => Ok(&value[2..]),
            value if value.len() == 40 => Ok(value),
            _ => Err(DomainError::InvalidArgument(
                "invalid validator id length".to_string(),
            )),
        }?;

        let result = hex::decode(hex_value).map_err(|_| {
            DomainError::InvalidArgument("Failed to deserialize validator id".to_string())
        })?;

        if result.len() != 20 {
            return Err(DomainError::InvalidArgument(format!(
                "Invalid validator id value {}",
                value
            )));
        }

        let mut id: [u8; 20] = [0; 20];
        id.copy_from_slice(&result[..]);
        Ok(Self(id))
    }
}

impl TryFrom<&String> for Address {
    type Error = DomainError;

    fn try_from(value: &String) -> Result<Self, Self::Error> {
        Address::try_from(value.as_str())
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_checksum())
    }
}

impl TryFrom<Value> for Address {
    type Error = DomainError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        let string = value.try_string().map_err(|err| {
            DomainError::InvalidArgument(format!("Value is not a string: {}", err))
        })?;

        Self::try_from(&string)
    }
}


mod ser {
    use hex::FromHex;
    use serde::{Deserialize, Deserializer};

    pub(super) fn from_str<'de, D>(deserializer: D) -> Result<[u8; 20], D::Error>
    where
        D: Deserializer<'de>,
    {
        let validator_id = String::deserialize(deserializer)?;
        if validator_id.is_empty() || validator_id.len() != 42 {
            return Err(serde::de::Error::custom(
                "invalid validator id length".to_string(),
            ));
        }

        <[u8; 20] as FromHex>::from_hex(&validator_id[2..]).map_err(serde::de::Error::custom)
    }
}
