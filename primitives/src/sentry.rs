use crate::{
    targeting::Rules,
    validator::Type as MessageType,
    validator::{ApproveState, Heartbeat, MessageTypes, NewState},
    BigNum, Channel, ChannelId, ValidatorId,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fmt, hash::Hash};

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LastApproved {
    /// NewState can be None if the channel is brand new
    pub new_state: Option<MessageResponse<NewState>>,
    /// ApproveState can be None if the channel is brand new
    pub approve_state: Option<MessageResponse<ApproveState>>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct MessageResponse<T: MessageType> {
    pub from: ValidatorId,
    pub received: DateTime<Utc>,
    pub msg: message::Message<T>,
}

pub mod message {
    use std::{convert::TryFrom, ops::Deref};

    use crate::validator::messages::*;
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
    #[serde(try_from = "MessageTypes", into = "MessageTypes")]
    pub struct Message<T: Type>(T);

    impl<T: Type> Message<T> {
        pub fn into_inner(self) -> T {
            self.0
        }
    }

    impl<T: Type> Deref for Message<T> {
        type Target = T;

        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

    impl<T: Type> TryFrom<MessageTypes> for Message<T> {
        type Error = MessageTypeError<T>;

        fn try_from(value: MessageTypes) -> Result<Self, Self::Error> {
            <T as TryFrom<MessageTypes>>::try_from(value).map(Self)
        }
    }

    impl<T: Type> Into<MessageTypes> for Message<T> {
        fn into(self) -> MessageTypes {
            self.0.into()
        }
    }
}

#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum Event {
    #[serde(rename_all = "camelCase")]
    Impression {
        publisher: ValidatorId,
        ad_unit: Option<String>,
        ad_slot: Option<String>,
        referrer: Option<String>,
    },
    Click {
        publisher: ValidatorId,
        ad_unit: Option<String>,
        ad_slot: Option<String>,
        referrer: Option<String>,
    },
    /// only the creator can send this event
    UpdateTargeting { targeting_rules: Rules },
    /// only the creator can send this event
    Close,
}

impl Event {
    pub fn is_click_event(&self) -> bool {
        matches!(self, Event::Click { .. })
    }

    pub fn is_impression_event(&self) -> bool {
        matches!(self, Event::Impression { .. })
    }
}

impl fmt::Display for Event {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Event::Impression { .. } => write!(f, "IMPRESSION"),
            Event::Click { .. } => write!(f, "CLICK"),
            Event::UpdateTargeting { .. } => write!(f, "UPDATE_TARGETING"),
            Event::Close => write!(f, "CLOSE"),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Earner {
    #[serde(rename = "publisher")]
    pub address: String,
    pub promilles: u64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventAggregate {
    pub channel_id: ChannelId,
    pub created: DateTime<Utc>,
    pub events: HashMap<String, AggregateEvents>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AggregateEvents {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_counts: Option<HashMap<ValidatorId, BigNum>>,
    pub event_payouts: HashMap<ValidatorId, BigNum>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelListResponse {
    pub channels: Vec<Channel>,
    pub total_pages: u64,
    pub total: u64,
    pub page: u64,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LastApprovedResponse {
    pub last_approved: Option<LastApproved>,
    /// None -> withHeartbeat=true wasn't passed
    /// Some(vec![]) (empty vec) or Some(heartbeats) - withHeartbeat=true was passed
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heartbeats: Option<Vec<MessageResponse<Heartbeat>>>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SuccessResponse {
    pub success: bool,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ValidatorMessage {
    pub from: ValidatorId,
    pub received: DateTime<Utc>,
    pub msg: MessageTypes,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ValidatorMessageResponse {
    pub validator_messages: Vec<ValidatorMessage>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct EventAggregateResponse {
    pub channel: Channel,
    pub events: Vec<EventAggregate>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct ValidationErrorResponse {
    pub status_code: u64,
    pub message: String,
    pub validation: Vec<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdvancedAnalyticsResponse {
    pub by_channel_stats: HashMap<ChannelId, HashMap<ChannelReport, HashMap<String, f64>>>,
    pub publisher_stats: HashMap<PublisherReport, HashMap<String, f64>>,
}

#[derive(Serialize, Deserialize, Debug, Hash, PartialEq, Eq, Clone)]
#[serde(rename_all = "camelCase")]
pub enum PublisherReport {
    AdUnit,
    AdSlot,
    AdSlotPay,
    Country,
    Hostname,
}

impl fmt::Display for PublisherReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            PublisherReport::AdUnit => write!(f, "reportPublisherToAdUnit"),
            PublisherReport::AdSlot => write!(f, "reportPublisherToAdSlot"),
            PublisherReport::AdSlotPay => write!(f, "reportPublisherToAdSlotPay"),
            PublisherReport::Country => write!(f, "reportPublisherToCountry"),
            PublisherReport::Hostname => write!(f, "reportPublisherToHostname"),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Hash, PartialEq, Eq, Clone)]
#[serde(rename_all = "camelCase")]
pub enum ChannelReport {
    AdUnit,
    Hostname,
    HostnamePay,
}

impl fmt::Display for ChannelReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            ChannelReport::AdUnit => write!(f, "reportPublisherToAdUnit"),
            ChannelReport::Hostname => write!(f, "reportChannelToHostname"),
            ChannelReport::HostnamePay => write!(f, "reportChannelToHostnamePay"),
        }
    }
}

pub mod channel_list {
    use crate::ValidatorId;
    use chrono::{serde::ts_seconds, DateTime, Utc};
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize)]
    pub struct ChannelListQuery {
        #[serde(default = "default_page")]
        pub page: u64,
        /// filters the list on `valid_until >= valid_until_ge`
        /// It should be the same timestamp format as the `Channel.valid_until`: **seconds**
        #[serde(with = "ts_seconds", default = "Utc::now", rename = "validUntil")]
        pub valid_until_ge: DateTime<Utc>,
        pub creator: Option<String>,
        /// filters the channels containing a specific validator if provided
        pub validator: Option<ValidatorId>,
    }

    #[derive(Debug, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct LastApprovedQuery {
        pub with_heartbeat: Option<String>,
    }

    fn default_page() -> u64 {
        0
    }
}

#[cfg(feature = "postgres")]
mod postgres {
    use super::{MessageResponse, ValidatorMessage};
    use crate::{
        sentry::EventAggregate,
        validator::{messages::Type as MessageType, MessageTypes},
    };
    use bytes::BytesMut;
    use postgres_types::{accepts, to_sql_checked, IsNull, Json, ToSql, Type};
    use serde::Deserialize;
    use std::convert::TryFrom;
    use tokio_postgres::{Error, Row};

    impl From<&Row> for EventAggregate {
        fn from(row: &Row) -> Self {
            Self {
                channel_id: row.get("channel_id"),
                created: row.get("created"),
                events: row.get::<_, Json<_>>("events").0,
            }
        }
    }

    impl From<&Row> for ValidatorMessage {
        fn from(row: &Row) -> Self {
            Self {
                from: row.get("from"),
                received: row.get("received"),
                msg: row.get::<_, Json<MessageTypes>>("msg").0,
            }
        }
    }

    impl<T> TryFrom<&Row> for MessageResponse<T>
    where
        T: MessageType,
        for<'de> T: Deserialize<'de>,
    {
        type Error = Error;

        fn try_from(row: &Row) -> Result<Self, Self::Error> {
            Ok(Self {
                from: row.get("from"),
                received: row.get("received"),
                // guard against mistakes from wrong Queries
                msg: row.try_get::<_, Json<_>>("msg")?.0,
            })
        }
    }

    impl ToSql for MessageTypes {
        fn to_sql(
            &self,
            ty: &Type,
            w: &mut BytesMut,
        ) -> Result<IsNull, Box<dyn std::error::Error + Sync + Send>> {
            Json(self).to_sql(ty, w)
        }

        accepts!(JSONB);
        to_sql_checked!();
    }
}
