use std::error::Error;
use std::fmt;
use std::ops::Deref;
use std::str::FromStr;

use chrono::serde::{ts_milliseconds, ts_milliseconds_option, ts_seconds};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize};
use serde_hex::{SerHex, StrictPfx};

use crate::{targeting::Rules, AdUnit, BigNum, EventSubmission, ValidatorDesc, ValidatorId};
use hex::{FromHex, FromHexError};

#[derive(Serialize, Deserialize, PartialEq, Eq, Copy, Clone, Hash)]
#[serde(transparent)]
pub struct ChannelId(
    #[serde(
        deserialize_with = "deserialize_channel_id",
        serialize_with = "SerHex::<StrictPfx>::serialize"
    )]
    [u8; 32],
);

impl ChannelId {
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for ChannelId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ChannelId({})", self)
    }
}

fn deserialize_channel_id<'de, D>(deserializer: D) -> Result<[u8; 32], D::Error>
where
    D: Deserializer<'de>,
{
    let channel_id = String::deserialize(deserializer)?;
    validate_channel_id(&channel_id).map_err(serde::de::Error::custom)
}

fn validate_channel_id(s: &str) -> Result<[u8; 32], FromHexError> {
    // strip `0x` prefix
    let hex = s.strip_prefix("0x").unwrap_or(s);
    // FromHex will make sure to check the length and match it to 32 bytes
    <[u8; 32] as FromHex>::from_hex(hex)
}

impl Deref for ChannelId {
    type Target = [u8; 32];

    fn deref(&self) -> &[u8; 32] {
        &self.0
    }
}

impl From<[u8; 32]> for ChannelId {
    fn from(array: [u8; 32]) -> Self {
        Self(array)
    }
}

impl AsRef<[u8]> for ChannelId {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl FromHex for ChannelId {
    type Error = FromHexError;

    fn from_hex<T: AsRef<[u8]>>(hex: T) -> Result<Self, Self::Error> {
        let array = hex::FromHex::from_hex(hex)?;

        Ok(Self(array))
    }
}

impl fmt::Display for ChannelId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "0x{}", hex::encode(self.0))
    }
}

impl FromStr for ChannelId {
    type Err = FromHexError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        validate_channel_id(s).map(ChannelId)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ChannelError {
    InvalidArgument(String),
    /// When the Adapter address is not listed in the `channel.spec.validators`
    /// which in terms means, that the adapter shouldn't handle this Channel
    AdapterNotIncluded,
    /// when `channel.valid_until` has passed (< now), the channel should be handled
    InvalidValidUntil(String),
    UnlistedValidator,
    UnlistedCreator,
    UnlistedAsset,
    MinimumDepositNotMet,
    MinimumValidatorFeeNotMet,
    FeeConstraintViolated,
}

impl fmt::Display for ChannelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ChannelError::InvalidArgument(error) => write!(f, "{}", error),
            ChannelError::AdapterNotIncluded => write!(f, "channel is not validated by us"),
            ChannelError::InvalidValidUntil(error) => write!(f, "{}", error),
            ChannelError::UnlistedValidator => write!(f, "validators are not in the whitelist"),
            ChannelError::UnlistedCreator => write!(f, "channel.creator is not whitelisted"),
            ChannelError::UnlistedAsset => write!(f, "channel.depositAsset is not whitelisted"),
            ChannelError::MinimumDepositNotMet => {
                write!(f, "channel.depositAmount is less than MINIMAL_DEPOSIT")
            }
            ChannelError::MinimumValidatorFeeNotMet => {
                write!(f, "channel validator fee is less than MINIMAL_FEE")
            }
            ChannelError::FeeConstraintViolated => {
                write!(f, "total fees <= deposit: fee constraint violated")
            }
        }
    }
}

impl Error for ChannelError {
    fn cause(&self) -> Option<&dyn Error> {
        None
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_channel_id_() {
        let hex_string = "061d5e2a67d0a9a10f1c732bca12a676d83f79663a396f7d87b3e30b9b411088";
        let prefixed_string = format!("0x{}", hex_string);

        let expected_id = ChannelId([
            0x06, 0x1d, 0x5e, 0x2a, 0x67, 0xd0, 0xa9, 0xa1, 0x0f, 0x1c, 0x73, 0x2b, 0xca, 0x12,
            0xa6, 0x76, 0xd8, 0x3f, 0x79, 0x66, 0x3a, 0x39, 0x6f, 0x7d, 0x87, 0xb3, 0xe3, 0x0b,
            0x9b, 0x41, 0x10, 0x88,
        ]);

        assert_eq!(ChannelId::from_str(hex_string).unwrap(), expected_id);
        assert_eq!(ChannelId::from_str(&prefixed_string).unwrap(), expected_id);
        assert_eq!(ChannelId::from_hex(hex_string).unwrap(), expected_id);

        let hex_value = serde_json::Value::String(hex_string.to_string());
        let prefixed_value = serde_json::Value::String(prefixed_string.clone());

        // Deserialization from JSON
        let de_hex_json =
            serde_json::from_value::<ChannelId>(hex_value.clone()).expect("Should deserialize");
        let de_prefixed_json = serde_json::from_value::<ChannelId>(prefixed_value.clone())
            .expect("Should deserialize");

        assert_eq!(de_hex_json, expected_id);
        assert_eq!(de_prefixed_json, expected_id);

        // Serialization to JSON
        let actual_serialized = serde_json::to_value(expected_id).expect("Should Serialize");
        // we don't expect any capitalization
        assert_eq!(
            actual_serialized,
            serde_json::Value::String(prefixed_string)
        )
    }
}

#[cfg(feature = "postgres")]
pub mod postgres {
    use super::ChannelId;
    use bytes::BytesMut;
    use hex::FromHex;
    use postgres_types::{accepts, to_sql_checked, FromSql, IsNull, ToSql, Type};
    use std::error::Error;
    use tokio_postgres::Row;

    impl<'a> FromSql<'a> for ChannelId {
        fn from_sql(ty: &Type, raw: &'a [u8]) -> Result<Self, Box<dyn Error + Sync + Send>> {
            let str_slice = <&str as FromSql>::from_sql(ty, raw)?;

            Ok(ChannelId::from_hex(&str_slice[2..])?)
        }

        accepts!(TEXT, VARCHAR);
    }

    impl From<&Row> for ChannelId {
        fn from(row: &Row) -> Self {
            row.get("id")
        }
    }

    impl ToSql for ChannelId {
        fn to_sql(
            &self,
            ty: &Type,
            w: &mut BytesMut,
        ) -> Result<IsNull, Box<dyn Error + Sync + Send>> {
            let string = format!("0x{}", hex::encode(self));

            <String as ToSql>::to_sql(&string, ty, w)
        }

        fn accepts(ty: &Type) -> bool {
            <String as ToSql>::accepts(ty)
        }

        to_sql_checked!();
    }
}
