use serde::{Deserialize, Serialize};
use std::{convert::TryFrom, fmt, str::FromStr};

use crate::{address::Error, targeting::Value, Address, BigNum, DomainError, ToETHChecksum, ToHex};

pub use messages::*;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[serde(transparent)]
pub struct ValidatorId(Address);

impl fmt::Debug for ValidatorId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ValidatorId({})", self.to_hex_prefixed())
    }
}

impl ValidatorId {
    pub fn as_bytes(&self) -> &[u8; 20] {
        self.0.as_bytes()
    }

    pub fn to_address(&self) -> Address {
        self.0
    }

    pub fn inner(&self) -> &[u8; 20] {
        &self.0.as_bytes()
    }

    /// To Hex non-`0x` prefixed string without **Checksum**ing the string
    /// For backwards compatibility
    /// TODO: Remove once we change all places this method is used at
    pub fn to_hex_non_prefix_string(&self) -> String {
        self.0.to_hex()
    }

    /// To Hex `0x` prefixed string **without** __Checksum__ing the string
    /// For backwards compatibility
    /// TODO: Remove once we change all places this method is used at
    pub fn to_hex_prefix_string(&self) -> String {
        self.0.to_hex_prefixed()
    }
}

impl ToETHChecksum for ValidatorId {}

impl From<&[u8; 20]> for ValidatorId {
    fn from(bytes: &[u8; 20]) -> Self {
        Self(Address::from(bytes))
    }
}

impl AsRef<[u8]> for ValidatorId {
    fn as_ref(&self) -> &[u8] {
        &self.0.as_ref()
    }
}

impl FromStr for ValidatorId {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Address::try_from(s).map(Self)
    }
}

impl TryFrom<&str> for ValidatorId {
    type Error = Error;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Address::try_from(value).map(Self)
    }
}

impl TryFrom<&String> for ValidatorId {
    type Error = Error;

    fn try_from(value: &String) -> Result<Self, Self::Error> {
        Self::try_from(value.as_str())
    }
}

impl fmt::Display for ValidatorId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_checksum())
    }
}

impl TryFrom<Value> for ValidatorId {
    type Error = DomainError;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        Address::try_from(value).map(Self)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ValidatorDesc {
    pub id: ValidatorId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fee_addr: Option<ValidatorId>,
    pub url: String,
    pub fee: BigNum,
}

/// Validator Message Types
pub mod messages {
    use std::{any::type_name, convert::TryFrom, fmt, marker::PhantomData};
    use thiserror::Error;

    use crate::BalancesMap;
    use chrono::{DateTime, Utc};
    use serde::{Deserialize, Serialize};

    #[derive(Error, Debug)]
    pub struct MessageTypeError<T: Type> {
        expected: PhantomData<T>,
        actual: String,
    }

    impl<T: Type> MessageTypeError<T> {
        pub fn for_actual<A: Type>(_actual: &A) -> Self {
            Self {
                expected: PhantomData::default(),
                actual: type_name::<A>().to_string(),
            }
        }
    }

    impl<T: Type> fmt::Display for MessageTypeError<T> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(
                f,
                "Expected {} message type but the actual is {}",
                type_name::<T>(),
                self.actual
            )
        }
    }

    pub trait Type:
        fmt::Debug
        + Into<MessageTypes>
        + TryFrom<MessageTypes, Error = MessageTypeError<Self>>
        + Clone
        + PartialEq
        + Eq
    {
    }

    impl Type for Accounting {}
    impl TryFrom<MessageTypes> for Accounting {
        type Error = MessageTypeError<Self>;

        fn try_from(value: MessageTypes) -> Result<Self, Self::Error> {
            match value {
                MessageTypes::ApproveState(msg) => Err(MessageTypeError::for_actual(&msg)),
                MessageTypes::NewState(msg) => Err(MessageTypeError::for_actual(&msg)),
                MessageTypes::RejectState(msg) => Err(MessageTypeError::for_actual(&msg)),
                MessageTypes::Heartbeat(msg) => Err(MessageTypeError::for_actual(&msg)),
                MessageTypes::Accounting(accounting) => Ok(accounting),
            }
        }
    }
    impl Into<MessageTypes> for Accounting {
        fn into(self) -> MessageTypes {
            MessageTypes::Accounting(self)
        }
    }

    impl Type for ApproveState {}
    impl TryFrom<MessageTypes> for ApproveState {
        type Error = MessageTypeError<Self>;

        fn try_from(value: MessageTypes) -> Result<Self, Self::Error> {
            match value {
                MessageTypes::NewState(msg) => Err(MessageTypeError::for_actual(&msg)),
                MessageTypes::RejectState(msg) => Err(MessageTypeError::for_actual(&msg)),
                MessageTypes::Heartbeat(msg) => Err(MessageTypeError::for_actual(&msg)),
                MessageTypes::Accounting(msg) => Err(MessageTypeError::for_actual(&msg)),
                MessageTypes::ApproveState(approve_state) => Ok(approve_state),
            }
        }
    }
    impl Into<MessageTypes> for ApproveState {
        fn into(self) -> MessageTypes {
            MessageTypes::ApproveState(self)
        }
    }

    impl Type for NewState {}
    impl TryFrom<MessageTypes> for NewState {
        type Error = MessageTypeError<Self>;

        fn try_from(value: MessageTypes) -> Result<Self, Self::Error> {
            match value {
                MessageTypes::ApproveState(msg) => Err(MessageTypeError::for_actual(&msg)),
                MessageTypes::RejectState(msg) => Err(MessageTypeError::for_actual(&msg)),
                MessageTypes::Heartbeat(msg) => Err(MessageTypeError::for_actual(&msg)),
                MessageTypes::Accounting(msg) => Err(MessageTypeError::for_actual(&msg)),
                MessageTypes::NewState(new_state) => Ok(new_state),
            }
        }
    }
    impl Into<MessageTypes> for NewState {
        fn into(self) -> MessageTypes {
            MessageTypes::NewState(self)
        }
    }

    impl Type for RejectState {}
    impl TryFrom<MessageTypes> for RejectState {
        type Error = MessageTypeError<Self>;

        fn try_from(value: MessageTypes) -> Result<Self, Self::Error> {
            match value {
                MessageTypes::ApproveState(msg) => Err(MessageTypeError::for_actual(&msg)),
                MessageTypes::NewState(msg) => Err(MessageTypeError::for_actual(&msg)),
                MessageTypes::Heartbeat(msg) => Err(MessageTypeError::for_actual(&msg)),
                MessageTypes::Accounting(msg) => Err(MessageTypeError::for_actual(&msg)),
                MessageTypes::RejectState(reject_state) => Ok(reject_state),
            }
        }
    }
    impl Into<MessageTypes> for RejectState {
        fn into(self) -> MessageTypes {
            MessageTypes::RejectState(self)
        }
    }

    impl Type for Heartbeat {}
    impl TryFrom<MessageTypes> for Heartbeat {
        type Error = MessageTypeError<Self>;

        fn try_from(value: MessageTypes) -> Result<Self, Self::Error> {
            match value {
                MessageTypes::ApproveState(msg) => Err(MessageTypeError::for_actual(&msg)),
                MessageTypes::NewState(msg) => Err(MessageTypeError::for_actual(&msg)),
                MessageTypes::RejectState(msg) => Err(MessageTypeError::for_actual(&msg)),
                MessageTypes::Accounting(msg) => Err(MessageTypeError::for_actual(&msg)),
                MessageTypes::Heartbeat(heartbeat) => Ok(heartbeat),
            }
        }
    }
    impl Into<MessageTypes> for Heartbeat {
        fn into(self) -> MessageTypes {
            MessageTypes::Heartbeat(self)
        }
    }
    #[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
    #[serde(rename_all = "camelCase")]
    pub struct Accounting {
        #[serde(rename = "lastEvAggr")]
        pub last_event_aggregate: DateTime<Utc>,
        pub balances_before_fees: BalancesMap,
        pub balances: BalancesMap,
    }

    #[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
    #[serde(rename_all = "camelCase")]
    pub struct ApproveState {
        pub state_root: String,
        pub signature: String,
        pub is_healthy: bool,
        #[serde(default)]
        pub exhausted: bool,
    }

    #[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
    #[serde(rename_all = "camelCase")]
    pub struct NewState {
        pub state_root: String,
        pub signature: String,
        pub balances: BalancesMap,
        #[serde(default)]
        pub exhausted: bool,
    }

    #[derive(Default, Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
    #[serde(rename_all = "camelCase")]
    pub struct RejectState {
        pub reason: String,
        pub state_root: String,
        pub signature: String,
        pub balances: Option<BalancesMap>,
        pub timestamp: Option<DateTime<Utc>>,
    }

    #[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
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

    #[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
    #[serde(tag = "type")]
    pub enum MessageTypes {
        ApproveState(ApproveState),
        NewState(NewState),
        RejectState(RejectState),
        Heartbeat(Heartbeat),
        Accounting(Accounting),
    }
}
#[cfg(feature = "postgres")]
pub mod postgres {
    use super::ValidatorId;
    use crate::ToETHChecksum;
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
            matches!(*ty, Type::TEXT | Type::VARCHAR)
        }
    }

    impl ToSql for ValidatorId {
        fn to_sql(
            &self,
            ty: &Type,
            w: &mut BytesMut,
        ) -> Result<IsNull, Box<dyn Error + Sync + Send>> {
            let string = self.to_checksum();

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
            let string = self.to_checksum();

            <String as ToSql>::to_sql_checked(&string, ty, out)
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn validator_id_is_checksummed_when_serialized() {
        let validator_id_checksum_str = "0xce07CbB7e054514D590a0262C93070D838bFBA2e";

        let validator_id =
            ValidatorId::try_from(validator_id_checksum_str).expect("Valid string was provided");
        let actual_json = serde_json::to_string(&validator_id).expect("Should serialize");
        let expected_json = format!(r#""{}""#, validator_id_checksum_str);
        assert_eq!(expected_json, actual_json);
    }
}
