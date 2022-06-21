use hex::{FromHex, FromHexError};
use serde::{Deserialize, Serialize, Serializer};
use std::{fmt, str::FromStr};
use thiserror::Error;

use crate::{targeting::Value, DomainError, ToETHChecksum};

#[derive(Debug, Error)]
pub enum Error {
    #[error("Expected prefix `0x`")]
    BadPrefix,
    #[error("Expected length of 40 without or 42 with a `0x` prefix")]
    Length,
    #[error("Invalid hex")]
    Hex(#[from] FromHexError),
}

#[derive(Deserialize, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(transparent)]
pub struct Address(#[serde(deserialize_with = "de::from_bytes_insensitive")] [u8; 20]);

impl Address {
    pub fn to_bytes(&self) -> [u8; 20] {
        self.0
    }

    pub fn as_bytes(&self) -> &[u8; 20] {
        &self.0
    }

    pub fn from_bytes(bytes: &[u8; 20]) -> Self {
        Self(*bytes)
    }

    /// Creates an address from a bytes slice.
    ///
    /// Returns `None` if bytes length != 20
    pub fn from_slice(slice: &[u8]) -> Option<Self> {
        if slice.len() != 20 {
            None
        } else {
            let mut address = [0u8; 20];
            address.copy_from_slice(slice);

            Some(Self(address))
        }
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

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_checksum())
    }
}

impl fmt::Debug for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Address({})", self.to_checksum())
    }
}

impl ToETHChecksum for Address {}

impl From<&[u8; 20]> for Address {
    fn from(bytes: &[u8; 20]) -> Self {
        Self(*bytes)
    }
}

impl From<[u8; 20]> for Address {
    fn from(bytes: [u8; 20]) -> Self {
        Self(bytes)
    }
}

impl AsRef<[u8]> for Address {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl AsRef<[u8; 20]> for Address {
    fn as_ref(&self) -> &[u8; 20] {
        &self.0
    }
}

impl FromStr for Address {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(parse_bytes(s, Prefix::Insensitive)?))
    }
}

impl TryFrom<&str> for Address {
    type Error = Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Ok(Self(parse_bytes(value, Prefix::Insensitive)?))
    }
}

/// When we have a string literal (&str) representation of the Address in the form of bytes.
/// Useful for creating static values from strings for testing, configuration, etc.
///
/// You can find a test setup example in the [`crate::test_util`] module.
///
/// # Example
/// ```
/// use once_cell::sync::Lazy;
/// use primitives::Address;
///
/// static ADDRESS_0: Lazy<Address> = Lazy::new(|| b"0x80690751969B234697e9059e04ed72195c3507fa".try_into().unwrap());
///
/// println!("Address: {}", *ADDRESS_0);
/// ```
impl TryFrom<&'static [u8; 42]> for Address {
    type Error = Error;

    fn try_from(value: &'static [u8; 42]) -> Result<Self, Self::Error> {
        Ok(Self(parse_bytes(value, Prefix::With)?))
    }
}

impl TryFrom<&String> for Address {
    type Error = Error;

    fn try_from(value: &String) -> Result<Self, Self::Error> {
        Self::try_from(value.as_str())
    }
}

impl TryFrom<&[u8]> for Address {
    type Error = Error;

    fn try_from(slice: &[u8]) -> Result<Self, Self::Error> {
        Ok(Self(parse_bytes(slice, Prefix::Insensitive)?))
    }
}

impl TryFrom<Value> for Address {
    type Error = DomainError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        let string = value.try_string().map_err(|err| {
            DomainError::InvalidArgument(format!("Value is not a string: {}", err))
        })?;

        Self::try_from(&string).map_err(|err| DomainError::InvalidArgument(err.to_string()))
    }
}

mod de {
    use super::{parse_bytes, Prefix};
    use serde::{Deserialize, Deserializer};

    /// Deserializes the bytes with our without a `0x` prefix (insensitive)
    pub(super) fn from_bytes_insensitive<'de, D>(deserializer: D) -> Result<[u8; 20], D::Error>
    where
        D: Deserializer<'de>,
    {
        let address = String::deserialize(deserializer)?;

        parse_bytes(address, Prefix::Insensitive).map_err(serde::de::Error::custom)
    }
}

pub enum Prefix {
    // with `0x` prefix
    With,
    // without `0x` prefix
    Without,
    /// Insensitive to a `0x` prefixed, it allows values with or without a prefix
    Insensitive,
}

pub fn parse_bytes<T: AsRef<[u8]>>(from: T, prefix: Prefix) -> Result<[u8; 20], Error> {
    let bytes = from.as_ref();

    let from_hex =
        |hex_bytes: &[u8]| <[u8; 20] as FromHex>::from_hex(hex_bytes).map_err(Error::Hex);

    // this length check guards against `panic!` when we call `slice.split_at()`
    match (prefix, bytes.len()) {
        (Prefix::With, 42) | (Prefix::Insensitive, 42) => match bytes.split_at(2) {
            (b"0x", hex_bytes) => from_hex(hex_bytes),
            _ => Err(Error::BadPrefix),
        },
        (Prefix::Without, 40) | (Prefix::Insensitive, 40) => from_hex(bytes),
        _ => Err(Error::Length),
    }
}

#[cfg(feature = "postgres")]
pub mod postgres {
    use super::Address;
    use crate::ToETHChecksum;
    use bytes::BytesMut;
    use std::error::Error;
    use tokio_postgres::types::{FromSql, IsNull, ToSql, Type};

    impl<'a> FromSql<'a> for Address {
        fn from_sql(ty: &Type, raw: &'a [u8]) -> Result<Self, Box<dyn Error + Sync + Send>> {
            let str_slice = <&str as FromSql>::from_sql(ty, raw)?;

            Ok(str_slice.parse()?)
        }

        fn accepts(ty: &Type) -> bool {
            matches!(*ty, Type::TEXT | Type::VARCHAR)
        }
    }

    impl ToSql for Address {
        fn to_sql(
            &self,
            ty: &Type,
            w: &mut BytesMut,
        ) -> Result<IsNull, Box<dyn Error + Sync + Send>> {
            self.to_checksum().to_sql(ty, w)
        }

        fn accepts(ty: &Type) -> bool {
            <String as ToSql>::accepts(ty)
        }

        fn to_sql_checked(
            &self,
            ty: &Type,
            out: &mut BytesMut,
        ) -> Result<IsNull, Box<dyn Error + Sync + Send>> {
            self.to_checksum().to_sql_checked(ty, out)
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_de_serialization_of_address() {
        let prefixed_checksum = "0x80690751969B234697e9059e04ed72195c3507fa";
        let prefixed_lower = prefixed_checksum.to_lowercase();
        let no_prefix_checksum = &prefixed_checksum[2..];
        let no_prefix_lower = no_prefix_checksum.to_lowercase();

        let address = prefixed_checksum.parse::<Address>().expect("Valid Address");

        // prefixed checksum
        {
            let json = serde_json::to_value(address).expect("Should serialize");

            assert_eq!(
                serde_json::Value::String(prefixed_checksum.to_string()),
                json
            );
            let from_json = serde_json::from_value(json).expect("Should deserialize");

            assert_eq!(address, from_json);
        }

        // prefixed lower
        assert_eq!(
            prefixed_lower.parse::<Address>().expect("Valid Address"),
            address
        );

        // no prefix checksum
        assert_eq!(
            no_prefix_checksum
                .parse::<Address>()
                .expect("Valid Address"),
            address
        );

        // no prefix lower case
        assert_eq!(
            no_prefix_lower.parse::<Address>().expect("Valid Address"),
            address
        );
    }
}
