use ethereum_types::U256;
use serde::{Deserialize, Serialize};
use std::fmt;

use crate::{Address, ChannelId, ValidatorId};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Channel {
    pub leader: ValidatorId,
    pub follower: ValidatorId,
    pub guardian: Address,
    pub token: Address,
    pub nonce: Nonce,
}

impl Channel {
    pub fn id(&self) -> ChannelId {
        use ethabi::{encode, Token};
        use tiny_keccak::{Hasher, Keccak};

        let tokens = [
            Token::Address(self.leader.as_bytes().into()),
            Token::Address(self.follower.as_bytes().into()),
            Token::Address(self.guardian.as_bytes().into()),
            Token::Address(self.token.as_bytes().into()),
            Token::FixedBytes(self.nonce.to_bytes().to_vec()),
        ];

        let mut channel_id = [0_u8; 32];
        let mut hasher = Keccak::v256();
        hasher.update(&encode(&tokens));
        hasher.finalize(&mut channel_id);

        ChannelId::from(channel_id)
    }
}

/// The nonce is an Unsigned 256 number
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Nonce(pub U256);

impl Nonce {
    /// In Big-Endian
    pub fn to_bytes(&self) -> [u8; 32] {
        // the impl of From<U256> uses BigEndian
        self.0.into()
    }
}

impl fmt::Display for Nonce {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0.to_string())
    }
}

impl fmt::Debug for Nonce {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Nonce({})", self.0.to_string())
    }
}

impl From<u64> for Nonce {
    fn from(value: u64) -> Self {
        Self(U256::from(value))
    }
}

impl From<u32> for Nonce {
    fn from(value: u32) -> Self {
        Self(U256::from(value))
    }
}

// The U256 implementation deserializes the value from a hex String value with a prefix `0x...`
// This is why we we need to impl it our selves
impl<'de> Deserialize<'de> for Nonce {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let string = String::deserialize(deserializer)?;

        U256::from_dec_str(&string)
            .map_err(serde::de::Error::custom)
            .map(Nonce)
    }
}

// The U256 implementation serializes the value as a hex String value with a prefix `0x...`
// This is why we we need to impl it our selves
impl Serialize for Nonce {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.to_string().serialize(serializer)
    }
}
#[cfg(test)]
mod test {
    use super::*;
    use serde_json::{from_value, to_value, Value};

    #[test]
    fn de_serializes_nonce() {
        let nonce_str = "12345";
        let json = Value::String(nonce_str.into());

        let nonce: Nonce = from_value(json.clone()).expect("Should deserialize a Nonce");
        let expected_nonce = Nonce::from(12345_u64);

        assert_eq!(&expected_nonce, &nonce);
        assert_eq!(json, to_value(nonce).expect("Should serialize a Nonce"));
        assert_eq!(nonce_str, &nonce.to_string());
        assert_eq!("Nonce(12345)", &format!("{:?}", nonce));
    }
}

// TODO: Postgres Channel
