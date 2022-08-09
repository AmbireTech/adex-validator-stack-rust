use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::{borrow::Borrow, fmt, str::FromStr};

use crate::{
    address::Error,
    targeting::Value,
    util::{api::Error as ApiUrlError, ApiUrl},
    Address, DomainError, ToETHChecksum, ToHex, UnifiedNum,
};

#[doc(inline)]
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

    pub fn to_address(self) -> Address {
        self.0
    }

    pub fn as_address(&self) -> &Address {
        &self.0
    }

    pub fn inner(&self) -> &[u8; 20] {
        self.0.as_bytes()
    }
}

impl ToETHChecksum for ValidatorId {}

impl From<&Address> for ValidatorId {
    fn from(address: &Address) -> Self {
        Self(*address)
    }
}

impl From<Address> for ValidatorId {
    fn from(address: Address) -> Self {
        Self(address)
    }
}

impl From<&Lazy<Address>> for ValidatorId {
    fn from(address: &Lazy<Address>) -> Self {
        // once for the reference of &Lazy into Lazy
        // and once for moving out of Lazy into Address
        Self(**address)
    }
}

impl From<&[u8; 20]> for ValidatorId {
    fn from(bytes: &[u8; 20]) -> Self {
        Self(Address::from(bytes))
    }
}

impl AsRef<[u8]> for ValidatorId {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
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
/// A Validator description which includes the identity, fee (pro milles) and the Sentry URL.
pub struct ValidatorDesc {
    pub id: ValidatorId,
    /// The validator fee in pro milles (per 1000)
    ///
    /// Each fee is calculated based on the payout for an event.
    ///
    /// payout * fee / 1000 = event fee payout
    pub fee: UnifiedNum,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// The address which will receive the fees
    pub fee_addr: Option<Address>,
    /// The url of the Validator where Sentry API is running
    pub url: String,
}

impl ValidatorDesc {
    /// Tries to create an [`ApiUrl`] from the `url` field.
    pub fn try_api_url(&self) -> Result<ApiUrl, ApiUrlError> {
        self.url.parse()
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Validator<T> {
    Leader(T),
    Follower(T),
}

impl<T> Validator<T> {
    pub fn into_inner(self) -> T {
        match self {
            Self::Leader(validator) => validator,
            Self::Follower(validator) => validator,
        }
    }
}

impl<T> Borrow<T> for Validator<T> {
    fn borrow(&self) -> &T {
        match self {
            Self::Leader(validator) => validator,
            Self::Follower(validator) => validator,
        }
    }
}

/// Validator Message Types
pub mod messages {
    use std::{any::type_name, fmt, marker::PhantomData};
    use thiserror::Error;

    use crate::balances::{Balances, BalancesState, CheckedState, UncheckedState};
    use chrono::{DateTime, Utc};
    use serde::{Deserialize, Serialize};

    #[derive(Error, Debug)]
    pub enum MessageError<T: Type> {
        #[error(transparent)]
        Balances(#[from] crate::balances::Error),
        #[error("Expected {} message type but the actual is {actual}", type_name::<T>(), )]
        Type {
            expected: PhantomData<T>,
            actual: String,
        },
    }

    impl<T: Type> MessageError<T> {
        pub fn for_actual<A: Type>(_actual: &A) -> Self {
            Self::Type {
                expected: PhantomData::default(),
                actual: type_name::<A>().to_string(),
            }
        }
    }

    pub trait Type:
        fmt::Debug
        + Into<MessageTypes>
        + TryFrom<MessageTypes, Error = MessageError<Self>>
        + Clone
        + PartialEq
        + Eq
    {
    }

    impl Type for ApproveState {}
    impl TryFrom<MessageTypes> for ApproveState {
        type Error = MessageError<Self>;

        fn try_from(value: MessageTypes) -> Result<Self, Self::Error> {
            match value {
                MessageTypes::NewState(msg) => Err(MessageError::for_actual(&msg)),
                MessageTypes::RejectState(msg) => Err(MessageError::for_actual(&msg)),
                MessageTypes::Heartbeat(msg) => Err(MessageError::for_actual(&msg)),
                MessageTypes::ApproveState(approve_state) => Ok(approve_state),
            }
        }
    }
    impl From<ApproveState> for MessageTypes {
        fn from(approve_state: ApproveState) -> Self {
            MessageTypes::ApproveState(approve_state)
        }
    }

    impl<S: BalancesState> Type for NewState<S> {}
    impl<S: BalancesState> TryFrom<MessageTypes> for NewState<S> {
        type Error = MessageError<Self>;

        fn try_from(value: MessageTypes) -> Result<Self, Self::Error> {
            match value {
                MessageTypes::ApproveState(msg) => Err(MessageError::for_actual(&msg)),
                MessageTypes::RejectState(msg) => Err(MessageError::for_actual(&msg)),
                MessageTypes::Heartbeat(msg) => Err(MessageError::for_actual(&msg)),
                MessageTypes::NewState(new_state) => {
                    let balances = S::from_unchecked(new_state.balances)?;

                    Ok(Self {
                        state_root: new_state.state_root,
                        signature: new_state.signature,
                        balances,
                    })
                }
            }
        }
    }

    impl<S: BalancesState> From<NewState<S>> for MessageTypes {
        fn from(new_state: NewState<S>) -> Self {
            MessageTypes::NewState(NewState {
                state_root: new_state.state_root,
                signature: new_state.signature,
                balances: new_state.balances.into_unchecked(),
            })
        }
    }

    impl<S: BalancesState> Type for RejectState<S> {}
    impl<S: BalancesState> TryFrom<MessageTypes> for RejectState<S> {
        type Error = MessageError<Self>;

        fn try_from(value: MessageTypes) -> Result<Self, Self::Error> {
            match value {
                MessageTypes::ApproveState(msg) => Err(MessageError::for_actual(&msg)),
                MessageTypes::NewState(msg) => Err(MessageError::for_actual(&msg)),
                MessageTypes::Heartbeat(msg) => Err(MessageError::for_actual(&msg)),
                MessageTypes::RejectState(reject_state) => {
                    let balances = reject_state.balances.map(S::from_unchecked).transpose()?;

                    Ok(Self {
                        reason: reject_state.reason,
                        state_root: reject_state.state_root,
                        signature: reject_state.signature,
                        balances,
                        timestamp: reject_state.timestamp,
                    })
                }
            }
        }
    }

    impl<S: BalancesState> From<RejectState<S>> for MessageTypes {
        fn from(reject_state: RejectState<S>) -> Self {
            MessageTypes::RejectState(RejectState {
                reason: reject_state.reason,
                state_root: reject_state.state_root,
                signature: reject_state.signature,
                balances: reject_state
                    .balances
                    .map(|balances| balances.into_unchecked()),
                timestamp: reject_state.timestamp,
            })
        }
    }

    impl Type for Heartbeat {}
    impl TryFrom<MessageTypes> for Heartbeat {
        type Error = MessageError<Self>;

        fn try_from(value: MessageTypes) -> Result<Self, Self::Error> {
            match value {
                MessageTypes::ApproveState(msg) => Err(MessageError::for_actual(&msg)),
                MessageTypes::NewState(msg) => Err(MessageError::for_actual(&msg)),
                MessageTypes::RejectState(msg) => Err(MessageError::for_actual(&msg)),
                MessageTypes::Heartbeat(heartbeat) => Ok(heartbeat),
            }
        }
    }

    impl From<Heartbeat> for MessageTypes {
        fn from(heartbeat: Heartbeat) -> Self {
            MessageTypes::Heartbeat(heartbeat)
        }
    }

    /// Generated by the follower when a [`NewState`]
    /// is generated by the leader and the state is signable and correct.
    #[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
    #[serde(rename_all = "camelCase")]
    pub struct ApproveState {
        pub state_root: String,
        pub signature: String,
        pub is_healthy: bool,
    }

    /// Generated by the [`Channel.leader`](crate::Channel::leader)
    /// on changed balances for the [`Channel`](crate::Channel).
    #[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
    #[serde(rename_all = "camelCase")]
    pub struct NewState<S: BalancesState> {
        pub state_root: String,
        pub signature: String,
        #[serde(flatten, bound = "S: BalancesState")]
        pub balances: Balances<S>,
    }

    impl NewState<UncheckedState> {
        pub fn try_checked(self) -> Result<NewState<CheckedState>, crate::balances::Error> {
            Ok(NewState {
                state_root: self.state_root,
                signature: self.signature,
                balances: self.balances.check()?,
            })
        }
    }

    /// Generated by the [`Channel.follower`] on:
    ///
    /// - Payout mismatch in the [`NewState`] between earner & spenders (i.e. their sum is not equal)
    /// - Invalid [`NewState`] root hash.
    /// - Failed verification of the expected signer ([`Channel.leader`])
    ///   with the proposed [`NewState`] signature and state root.
    /// - Invalid state transition (balances should always go up)
    /// - [`NewState`] is unsignable because the health is below the [`Config.health_unsignable_promilles`]
    ///
    /// [`Channel.follower`]: crate::Channel::follower
    /// [`Channel.leader`]: crate::Channel::leader
    /// [`Config.health_unsignable_promilles`]: crate::Config::health_unsignable_promilles
    #[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
    #[serde(rename_all = "camelCase")]
    pub struct RejectState<S: BalancesState> {
        pub reason: String,
        pub state_root: String,
        pub signature: String,
        #[serde(flatten, bound = "S: BalancesState")]
        pub balances: Option<Balances<S>>,
        /// The timestamp when the [`NewState`] was rejected.
        pub timestamp: DateTime<Utc>,
    }

    /// Heartbeat sent to each [`Channel`] validator by the
    /// other validator of the [`Channel`].
    ///
    /// The Heartbeat is sent on regular intervals every [`Config.heartbeat_time`].
    ///
    /// [`Channel`]: crate::Channel
    /// [`Config.heartbeat_time`]: crate::Config::heartbeat_time
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

    /// The message types used by validator.
    #[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
    #[serde(tag = "type")]
    pub enum MessageTypes {
        ApproveState(ApproveState),
        NewState(NewState<UncheckedState>),
        RejectState(RejectState<UncheckedState>),
        Heartbeat(Heartbeat),
    }
}
#[cfg(feature = "postgres")]
mod postgres {
    use super::ValidatorId;
    use crate::ToETHChecksum;
    use bytes::BytesMut;
    use std::error::Error;
    use tokio_postgres::types::{FromSql, IsNull, ToSql, Type};

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
