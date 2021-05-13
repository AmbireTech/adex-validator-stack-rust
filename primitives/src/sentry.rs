use crate::{
    targeting::Rules,
    validator::{ApproveState, Heartbeat, MessageTypes, NewState, Type as MessageType},
    Address, BalancesMap, BigNum, Channel, ChannelId, ValidatorId, IPFS,
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
        pub fn new(message: T) -> Self {
            Self(message)
        }

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

    impl<T: Type> From<Message<T>> for MessageTypes {
        fn from(message: Message<T>) -> Self {
            message.0.into()
        }
    }

    #[cfg(test)]
    mod test {
        use super::*;
        use crate::sentry::MessageResponse;
        use chrono::{TimeZone, Utc};
        use serde_json::{from_value, json, to_value};

        #[test]
        fn de_serialization_of_a_message() {
            let approve_state_message = json!({
                "from":"0x2892f6C41E0718eeeDd49D98D648C789668cA67d",
                "msg": {
                    "type":"ApproveState",
                    "stateRoot":"4739522efc1e81499541621759dadb331eaf08829d6a3851b4b654dfaddc9935",
                    "signature":"0x00128a39b715e87475666c3220fc0400bf34a84d24f77571d2b4e1e88b141d52305438156e526ff4fe96b7a13e707ab2f6f3ca00bd928dabc7f516b56cfe6fd61c",
                    "isHealthy":true,
                    "exhausted":false
                },
                "received":"2021-01-05T14:00:48.549Z"
            });

            let actual_message: MessageResponse<ApproveState> =
                from_value(approve_state_message.clone()).expect("Should deserialize");
            let expected_message = MessageResponse {
                from: "0x2892f6C41E0718eeeDd49D98D648C789668cA67d".parse().expect("Valid ValidatorId"),
                received: Utc.ymd(2021, 1, 5).and_hms_milli(14,0,48, 549),
                msg: Message::new(ApproveState {
                    state_root: "4739522efc1e81499541621759dadb331eaf08829d6a3851b4b654dfaddc9935".to_string(),
                    signature: "0x00128a39b715e87475666c3220fc0400bf34a84d24f77571d2b4e1e88b141d52305438156e526ff4fe96b7a13e707ab2f6f3ca00bd928dabc7f516b56cfe6fd61c".to_string(),
                    is_healthy: true,
                    exhausted: false,
                }),
            };

            pretty_assertions::assert_eq!(expected_message, actual_message);
            pretty_assertions::assert_eq!(
                to_value(expected_message).expect("should serialize"),
                approve_state_message
            );
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Event {
    #[serde(rename_all = "camelCase")]
    Impression {
        publisher: Address,
        ad_unit: Option<IPFS>,
        ad_slot: Option<IPFS>,
        referrer: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    Click {
        publisher: Address,
        ad_unit: Option<IPFS>,
        ad_slot: Option<IPFS>,
        referrer: Option<String>,
    },
    /// only the creator can send this event
    #[serde(rename_all = "camelCase")]
    UpdateTargeting { targeting_rules: Rules },
    /// Closes the `Campaign`
    /// only the creator can send this event
    #[serde(rename_all = "camelCase")]
    Close,
    /// TODO: AIP#61 Check and explain who can send this event as well as when it can be received
    /// A map of earners which gets merged in the `spender::Aggregate`
    /// NOTE: Does **not** contain any fees!
    /// This even can be used to pay to yourself, but this is irrelevant as it's your funds you are paying yourself.
    #[serde(rename_all = "camelCase")]
    Pay { payout: BalancesMap },
}

impl Event {
    pub fn is_click_event(&self) -> bool {
        matches!(self, Event::Click { .. })
    }

    pub fn is_impression_event(&self) -> bool {
        matches!(self, Event::Impression { .. })
    }

    pub fn as_str(&self) -> &str {
        self.as_ref()
    }
}

impl AsRef<str> for Event {
    fn as_ref(&self) -> &str {
        match *self {
            Event::Impression { .. } => "IMPRESSION",
            Event::Click { .. } => "CLICK",
            Event::UpdateTargeting { .. } => "UPDATE_TARGETING",
            Event::Close => "CLOSE",
            Event::Pay { .. } => "PAY",
        }
    }
}

impl fmt::Display for Event {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_ref())
    }
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
    pub event_counts: Option<HashMap<Address, BigNum>>,
    pub event_payouts: HashMap<Address, BigNum>,
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

pub mod accounting {
    use crate::{
        balances_map::UnifiedMap,
        ChannelId
    };
    use chrono::{DateTime, Utc};
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
    #[serde(rename_all = "camelCase")]
    pub struct Accounting {
        pub channel_id: ChannelId,
        #[serde(flatten)]
        pub balances: Balances,
        pub updated_at: DateTime<Utc>,
    }

    #[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
    #[serde(rename_all = "camelCase")]
    pub struct Balances {
        pub earners: UnifiedMap,
        pub spenders: UnifiedMap,
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

pub mod campaign_create {
    use chrono::{serde::ts_milliseconds, DateTime, Utc};
    use serde::{Deserialize, Serialize};
    use serde_with::with_prefix;

    use crate::{
        campaign::{Active, PricingBounds, Validators},
        channel_v5::Channel,
        targeting::Rules,
        AdUnit, Address, Campaign, CampaignId, EventSubmission, UnifiedNum,
    };

    // use the same prefix for Active
    with_prefix!(prefix_active "active_");

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    /// All fields are present except the `CampaignId` which is randomly created
    /// This struct defines the Body of the request (in JSON)
    pub struct CreateCampaign {
        pub channel: Channel,
        pub creator: Address,
        pub budget: UnifiedNum,
        pub validators: Validators,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub title: Option<String>,
        /// Event pricing bounds
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub pricing_bounds: Option<PricingBounds>,
        /// EventSubmission object, applies to event submission (POST /channel/:id/events)
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub event_submission: Option<EventSubmission>,
        /// An array of AdUnit (optional)
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        pub ad_units: Vec<AdUnit>,
        #[serde(default)]
        pub targeting_rules: Rules,
        /// A millisecond timestamp of when the campaign was created
        #[serde(with = "ts_milliseconds")]
        pub created: DateTime<Utc>,
        /// A millisecond timestamp representing the time you want this campaign to become active (optional)
        /// Used by the AdViewManager & Targeting AIP#31
        #[serde(flatten, with = "prefix_active")]
        pub active: Active,
    }

    impl CreateCampaign {
        /// Creates the new `Campaign` with randomly generated `CampaignId`
        pub fn into_campaign(self) -> Campaign {
            Campaign {
                id: CampaignId::new(),
                channel: self.channel,
                creator: self.creator,
                budget: self.budget,
                validators: self.validators,
                title: self.title,
                pricing_bounds: self.pricing_bounds,
                event_submission: self.event_submission,
                ad_units: self.ad_units,
                targeting_rules: self.targeting_rules,
                created: self.created,
                active: self.active,
            }
        }
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

#[cfg(test)]
mod test {
    use super::*;
    use crate::util::tests::prep_db::{ADDRESSES, DUMMY_IPFS};
    use serde_json::json;

    #[test]
    pub fn de_serialize_events() {
        let click = Event::Click {
            publisher: ADDRESSES["publisher"],
            ad_unit: Some(DUMMY_IPFS[0].clone()),
            ad_slot: Some(DUMMY_IPFS[1].clone()),
            referrer: Some("some_referrer".to_string()),
        };

        let click_json = json!({
            "type": "CLICK",
            "publisher": "0xB7d3F81E857692d13e9D63b232A90F4A1793189E",
            "adUnit": "QmcUVX7fvoLMM93uN2bD3wGTH8MXSxeL8hojYfL2Lhp7mR",
            "adSlot": "Qmasg8FrbuSQpjFu3kRnZF9beg8rEBFrqgi1uXDRwCbX5f",
            "referrer": "some_referrer"
        });

        pretty_assertions::assert_eq!(
            click_json,
            serde_json::to_value(click).expect("should serialize")
        );
    }
}
