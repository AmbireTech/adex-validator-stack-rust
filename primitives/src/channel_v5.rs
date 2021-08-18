use ethereum_types::U256;
use serde::{Deserialize, Serialize};
use std::fmt;

use crate::{Address, ChannelId, Validator, ValidatorId};

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

    pub fn find_validator(&self, validator: ValidatorId) -> Option<Validator<ValidatorId>> {
        match (self.leader, self.follower) {
            (leader, _) if leader == validator => Some(Validator::Leader(leader)),
            (_, follower) if follower == validator => Some(Validator::Follower(follower)),
            _ => None,
        }
    }
}

/// The nonce is an Unsigned 256 number
#[derive(Clone, Copy, PartialEq, Eq)]
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

#[cfg(feature = "postgres")]
mod postgres {
    use super::Channel;
    use bytes::BytesMut;
    use postgres_types::{accepts, to_sql_checked, FromSql, IsNull, Json, ToSql, Type};
    use std::error::Error;

    impl<'a> FromSql<'a> for Channel {
        fn from_sql(ty: &Type, raw: &'a [u8]) -> Result<Self, Box<dyn Error + Sync + Send>> {
            let json: Json<Channel> = FromSql::from_sql(ty, raw)?;

            Ok(json.0)
        }

        accepts!(JSONB);
    }

    impl ToSql for Channel {
        fn to_sql(
            &self,
            ty: &Type,
            w: &mut BytesMut,
        ) -> Result<IsNull, Box<dyn Error + Sync + Send>> {
            Json(self).to_sql(ty, w)
        }

        accepts!(JSONB);
        to_sql_checked!();
    }

    #[cfg(test)]
    mod test {
        use crate::util::tests::prep_db::{postgres::POSTGRES_POOL, DUMMY_CAMPAIGN};
        #[tokio::test]
        async fn channel_to_from_sql() {
            let client = POSTGRES_POOL.get().await.unwrap();

            let channel = DUMMY_CAMPAIGN.channel.clone();
            let sql_type = "JSONB";

            // from SQL
            {
                let channel_json = serde_json::to_string(&channel).expect("Should serialize");

                let rows = client
                    .query(&*format!("SELECT '{}'::{}", channel_json, sql_type), &[])
                    .await
                    .unwrap();
                let result = rows[0].get(0);

                assert_eq!(&channel, &result);
            }

            // to SQL
            {
                let rows = client
                    .query(&*format!("SELECT $1::{}", sql_type), &[&channel])
                    .await
                    .unwrap();
                let result = rows[0].get(0);
                assert_eq!(&channel, &result);
            }
        }
    }
}
