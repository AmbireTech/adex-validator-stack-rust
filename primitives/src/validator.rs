use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_hex::{SerHex, StrictPfx};
use std::fmt;

use crate::{BalancesMap, BigNum, DomainError, ToETHChecksum};
use std::convert::TryFrom;

#[derive(Debug)]
pub enum ValidatorError {
    None,
    InvalidRootHash,
    InvalidSignature,
    InvalidTransition,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(transparent)]
pub struct ValidatorId(#[serde(with = "SerHex::<StrictPfx>")] [u8; 20]);

impl ValidatorId {
    pub fn inner(&self) -> &[u8; 20] {
        &self.0
    }

    /// To Hex non-`0x` prefixed string without **Checksum**ing the string
    pub fn to_hex_non_prefix_string(&self) -> String {
        hex::encode(self.0)
    }

    /// To Hex `0x` prefixed string **without** __Checksum__ing the string
    pub fn to_hex_prefix_string(&self) -> String {
        format!("0x{}", self.to_hex_non_prefix_string())
    }

    // To Hex `0x` prefixed string **with** **Checksum**ing the string
    pub fn to_hex_checksummed_string(&self) -> String {
        eth_checksum::checksum(&format!("0x{}", self.to_hex_non_prefix_string()))
    }
}

impl ToETHChecksum for ValidatorId {}

impl From<&[u8; 20]> for ValidatorId {
    fn from(bytes: &[u8; 20]) -> Self {
        Self(*bytes)
    }
}

impl AsRef<[u8]> for ValidatorId {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl TryFrom<&str> for ValidatorId {
    type Error = DomainError;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let hex_value = if value.len() == 42 {
            &value[2..]
        } else {
            value
        };

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

impl TryFrom<&String> for ValidatorId {
    type Error = DomainError;
    fn try_from(value: &String) -> Result<Self, Self::Error> {
        ValidatorId::try_from(value.as_str())
    }
}

impl fmt::Display for ValidatorId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_checksum())
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ValidatorDesc {
    pub id: ValidatorId,
    pub fee_addr: Option<ValidatorId>,
    pub url: String,
    pub fee: BigNum,
}

// Validator Message Types

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Accounting {
    #[serde(rename = "lastEvAggr")]
    pub last_event_aggregate: DateTime<Utc>,
    pub balances_before_fees: BalancesMap,
    pub balances: BalancesMap,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ApproveState {
    pub state_root: String,
    pub signature: String,
    pub is_healthy: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct NewState {
    pub state_root: String,
    pub signature: String,
    pub balances: BalancesMap,
}

#[derive(Default, Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct RejectState {
    pub reason: String,
    pub state_root: String,
    pub signature: String,
    pub balances: Option<BalancesMap>,
    pub timestamp: Option<DateTime<Utc>>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Heartbeat {
    pub signature: String,
    pub state_root: String,
    pub timestamp: DateTime<Utc>,
}

impl Heartbeat {
    pub fn new(signature: String, state_root: String) -> Self {
        Self {
            signature,
            state_root,
            timestamp: Utc::now(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type")]
pub enum MessageTypes {
    ApproveState(ApproveState),
    NewState(NewState),
    RejectState(RejectState),
    Heartbeat(Heartbeat),
    Accounting(Accounting),
}

#[cfg(feature = "postgres")]
pub mod postgres {
    use super::ValidatorId;
    use bytes::BytesMut;
    use postgres_types::{FromSql, IsNull, ToSql, Type};
    use std::convert::TryFrom;
    use std::error::Error;

    impl<'a> FromSql<'a> for ValidatorId {
        fn from_sql(ty: &Type, raw: &'a [u8]) -> Result<Self, Box<dyn Error + Sync + Send>> {
            let str_slice = <&str as FromSql>::from_sql(ty, raw)?;

            // FromHex::from_hex for fixed-sized arrays will guard against the length of the string!
            Ok(ValidatorId::try_from(str_slice)?)
        }

        fn accepts(ty: &Type) -> bool {
            match *ty {
                Type::TEXT | Type::VARCHAR => true,
                _ => false,
            }
        }
    }

    impl ToSql for ValidatorId {
        fn to_sql(
            &self,
            ty: &Type,
            w: &mut BytesMut,
        ) -> Result<IsNull, Box<dyn Error + Sync + Send>> {
            let string = format!("0x{}", self.to_hex_non_prefix_string());

            <String as ToSql>::to_sql(&string, ty, w)
        }

        fn accepts(ty: &Type) -> bool {
            <String as ToSql>::accepts(ty)
        }

        fn to_sql_checked(
            &self,
            ty: &Type,
            out: &mut BytesMut,
        ) -> Result<IsNull, Box<dyn Error + Sync + Send>> {
            let string = format!("0x{}", self.to_hex_non_prefix_string());

            <String as ToSql>::to_sql_checked(&string, ty, out)
        }
    }
}
