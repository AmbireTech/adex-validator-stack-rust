use crate::validator::MessageTypes;
use crate::{BigNum, Channel, ChannelId, ValidatorId};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::hash::Hash;

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LastApproved {
    /// NewState can be None if the channel is brand new
    pub new_state: Option<NewStateValidatorMessage>,
    /// ApproveState can be None if the channel is brand new
    pub approve_state: Option<ApproveStateValidatorMessage>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct NewStateValidatorMessage {
    pub from: ValidatorId,
    pub received: DateTime<Utc>,
    pub msg: MessageTypes,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct ApproveStateValidatorMessage {
    pub from: ValidatorId,
    pub received: DateTime<Utc>,
    pub msg: MessageTypes,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct HeartbeatValidatorMessage {
    pub from: ValidatorId,
    pub received: DateTime<Utc>,
    pub msg: MessageTypes,
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
    ImpressionWithCommission {
        earners: Vec<Earner>,
    },
    /// only the creator can send this event
    UpdateImpressionPrice {
        price: BigNum,
    },
    /// only the creator can send this event
    Pay {
        outputs: HashMap<String, BigNum>,
    },
    /// only the creator can send this event
    PauseChannel,
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
            Event::ImpressionWithCommission { .. } => write!(f, "IMPRESSION_WITH_COMMMISION"),
            Event::UpdateImpressionPrice { .. } => write!(f, "UPDATE_IMPRESSION_PRICE"),
            Event::Pay { .. } => write!(f, "PAY"),
            Event::PauseChannel => write!(f, "PAUSE_CHANNEL"),
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
    pub heartbeats: Option<Vec<HeartbeatValidatorMessage>>,
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
    use super::{
        ApproveStateValidatorMessage, HeartbeatValidatorMessage, NewStateValidatorMessage,
        ValidatorMessage,
    };
    use crate::sentry::EventAggregate;
    use crate::validator::MessageTypes;
    use bytes::BytesMut;
    use postgres_types::{accepts, to_sql_checked, IsNull, Json, ToSql, Type};
    use std::error::Error;
    use tokio_postgres::Row;

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

    impl From<&Row> for ApproveStateValidatorMessage {
        fn from(row: &Row) -> Self {
            Self {
                from: row.get("from"),
                received: row.get("received"),
                msg: row.get::<_, Json<MessageTypes>>("msg").0,
            }
        }
    }

    impl From<&Row> for NewStateValidatorMessage {
        fn from(row: &Row) -> Self {
            Self {
                from: row.get("from"),
                received: row.get("received"),
                msg: row.get::<_, Json<MessageTypes>>("msg").0,
            }
        }
    }

    impl From<&Row> for HeartbeatValidatorMessage {
        fn from(row: &Row) -> Self {
            Self {
                from: row.get("from"),
                received: row.get("received"),
                msg: row.get::<_, Json<MessageTypes>>("msg").0,
            }
        }
    }

    impl ToSql for MessageTypes {
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
}
