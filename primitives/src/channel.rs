use std::{fmt, ops::Deref, str::FromStr};

use ethereum_types::U256;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use hex::{FromHex, FromHexError};

use crate::{Address, ToHex, Validator, ValidatorId};

#[derive(Deserialize, PartialEq, Eq, Copy, Clone, Hash)]
#[serde(transparent)]
pub struct ChannelId(#[serde(deserialize_with = "deserialize_channel_id")] [u8; 32]);

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

impl Serialize for ChannelId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0.to_hex_prefixed())
    }
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

#[derive(Serialize, Deserialize, Debug, Copy, Clone, PartialEq, Eq, Hash)]
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

    pub fn find_validator(&self, validator: ValidatorId) -> Option<Validator<ValidatorId>> {
        match (self.leader, self.follower) {
            (leader, _) if leader == validator => Some(Validator::Leader(leader)),
            (_, follower) if follower == validator => Some(Validator::Follower(follower)),
            _ => None,
        }
    }
}

/// The nonce is an Unsigned 256 number
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct Nonce(pub U256);

impl Nonce {
    /// In Big-Endian
    pub fn to_bytes(self) -> [u8; 32] {
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
        write!(f, "Nonce({})", self.0)
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
            serde_json::from_value::<ChannelId>(hex_value).expect("Should deserialize");
        let de_prefixed_json =
            serde_json::from_value::<ChannelId>(prefixed_value).expect("Should deserialize");

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

#[cfg(feature = "postgres")]
mod postgres {
    use super::{Channel, ChannelId, Nonce};
    use bytes::BytesMut;
    use hex::FromHex;
    use std::error::Error;
    use tokio_postgres::types::{accepts, to_sql_checked, FromSql, IsNull, ToSql, Type};
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
            let string = self.to_string();

            <String as ToSql>::to_sql(&string, ty, w)
        }

        fn accepts(ty: &Type) -> bool {
            <String as ToSql>::accepts(ty)
        }

        to_sql_checked!();
    }

    impl<'a> FromSql<'a> for Nonce {
        fn from_sql(ty: &Type, raw: &'a [u8]) -> Result<Self, Box<dyn Error + Sync + Send>> {
            let nonce_string = String::from_sql(ty, raw)?;

            Ok(serde_json::from_value(serde_json::Value::String(
                nonce_string,
            ))?)
        }

        accepts!(VARCHAR);
    }

    impl ToSql for Nonce {
        fn to_sql(
            &self,
            ty: &Type,
            w: &mut BytesMut,
        ) -> Result<IsNull, Box<dyn Error + Sync + Send>> {
            self.0.to_string().to_sql(ty, w)
        }

        accepts!(VARCHAR);
        to_sql_checked!();
    }

    impl From<&Row> for Channel {
        fn from(row: &Row) -> Self {
            Self {
                leader: row.get("leader"),
                follower: row.get("follower"),
                guardian: row.get("guardian"),
                token: row.get("token"),
                nonce: row.get("nonce"),
            }
        }
    }

    #[cfg(test)]
    mod test {
        use crate::{channel::Nonce, postgres::POSTGRES_POOL};
        #[tokio::test]
        async fn nonce_to_from_sql() {
            let client = POSTGRES_POOL.get().await.unwrap();

            let nonce = Nonce::from(123_456_789_u64);
            let sql_type = "VARCHAR";

            // from SQL
            {
                let row_nonce = client
                    .query_one(&*format!("SELECT '{}'::{}", nonce, sql_type), &[])
                    .await
                    .unwrap()
                    .get(0);

                assert_eq!(&nonce, &row_nonce);
            }

            // to SQL
            {
                let row_nonce = client
                    .query_one(&*format!("SELECT $1::{}", sql_type), &[&nonce])
                    .await
                    .unwrap()
                    .get(0);
                assert_eq!(&nonce, &row_nonce);
            }
        }
    }
}
