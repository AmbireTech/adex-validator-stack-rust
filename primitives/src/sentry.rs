use crate::{
    analytics::OperatingSystem,
    balances::BalancesState,
    spender::Spender,
    validator::{ApproveState, Heartbeat, MessageTypes, NewState, Type as MessageType},
    Address, Balances, BigNum, CampaignId, Channel, ChannelId, UnifiedNum, ValidatorId, IPFS,
};
use bytes::BytesMut;
use chrono::{DateTime, Datelike, TimeZone, Timelike, Utc};
use postgres_types::{accepts, to_sql_checked, FromSql, IsNull, ToSql, Type};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, error::Error, fmt, hash::Hash};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
/// Channel Accounting response
/// A collection of all `Accounting`s for a specific `Channel`
pub struct AccountingResponse<S: BalancesState> {
    #[serde(flatten, bound = "S: BalancesState")]
    pub balances: Balances<S>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LastApproved<S: BalancesState> {
    /// NewState can be None if the channel is brand new
    #[serde(bound = "S: BalancesState")]
    pub new_state: Option<MessageResponse<NewState<S>>>,
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
    pub struct Message<T: Type>(pub T);

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
        type Error = MessageError<T>;

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
                    "isHealthy":true
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
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Hash, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAnalytics {
    pub time: DateHour,
    pub campaign_id: CampaignId,
    pub ad_unit: Option<IPFS>,
    pub ad_slot: Option<IPFS>,
    pub ad_slot_type: Option<String>,
    pub advertiser: Address,
    pub publisher: Address,
    pub hostname: Option<String>,
    pub country: Option<String>,
    pub os_name: OperatingSystem,
    pub event_type: String,
    pub amount_to_add: UnifiedNum,
    pub count_to_add: i32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Analytics {
    pub time: DateHour,
    pub campaign_id: CampaignId,
    pub ad_unit: Option<IPFS>,
    pub ad_slot: Option<IPFS>,
    pub ad_slot_type: Option<String>,
    pub advertiser: Address,
    pub publisher: Address,
    pub hostname: Option<String>,
    pub country: Option<String>,
    pub os_name: OperatingSystem,
    pub event_type: String,
    pub payout_amount: UnifiedNum,
    pub payout_count: i32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Hash, Eq)]
pub struct DateHour {
    pub date: DateTime<Utc>,
    pub hour: u32,
}

impl<'a> FromSql<'a> for DateHour {
    fn from_sql(ty: &Type, raw: &'a [u8]) -> Result<Self, Box<dyn Error + Sync + Send>> {
        let date_time = <DateTime<Utc> as FromSql>::from_sql(ty, raw)?;
        assert_eq!(date_time.time().minute(), 0);
        assert_eq!(date_time.time().second(), 0);
        assert_eq!(date_time.time().nanosecond(), 0);
        Ok(Self {
            date: date_time,
            hour: date_time.hour(),
        })
    }
    accepts!(TIMESTAMPTZ);
}

impl ToSql for DateHour {
    fn to_sql(&self, ty: &Type, w: &mut BytesMut) -> Result<IsNull, Box<dyn Error + Sync + Send>> {
        Utc.ymd(self.date.year(), self.date.month(), self.date.day())
            .and_hms(self.hour, 0, 0)
            .to_sql(ty, w)
    }

    accepts!(TIMESTAMPTZ);
    to_sql_checked!();
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

#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Pagination {
    pub total_pages: u64,
    pub page: u64,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct LastApprovedResponse<S: BalancesState> {
    #[serde(bound = "S: BalancesState")]
    pub last_approved: Option<LastApproved<S>>,
    /// None -> withHeartbeat=true wasn't passed
    /// Some(vec![]) (empty vec) or Some(heartbeats) - withHeartbeat=true was passed
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heartbeats: Option<Vec<MessageResponse<Heartbeat>>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LastApprovedQuery {
    pub with_heartbeat: Option<String>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SuccessResponse {
    pub success: bool,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SpenderResponse {
    pub spender: Spender,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
pub struct AllSpendersResponse {
    pub spenders: HashMap<Address, Spender>,
    #[serde(flatten)]
    pub pagination: Pagination,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AllSpendersQuery {
    // default is `u64::default()` = `0`
    #[serde(default)]
    pub page: u64,
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
    use crate::{Channel, ValidatorId};
    use serde::{Deserialize, Serialize};

    use super::Pagination;

    #[derive(Debug, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct ChannelListResponse {
        pub channels: Vec<Channel>,
        #[serde(flatten)]
        pub pagination: Pagination,
    }

    #[derive(Debug, Serialize, Deserialize)]
    pub struct ChannelListQuery {
        #[serde(default)]
        // default is `u64::default()` = `0`
        pub page: u64,
        pub creator: Option<String>,
        /// filters the channels containing a specific validator if provided
        pub validator: Option<ValidatorId>,
    }
}

pub mod campaign {
    use crate::{Address, Campaign, ValidatorId};
    use chrono::{serde::ts_seconds, DateTime, Utc};
    use serde::{Deserialize, Serialize};

    use super::Pagination;

    #[derive(Debug, Serialize, Deserialize)]
    #[serde(rename_all = "camelCase")]
    pub struct CampaignListResponse {
        pub campaigns: Vec<Campaign>,
        #[serde(flatten)]
        pub pagination: Pagination,
    }

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    pub struct CampaignListQuery {
        #[serde(default)]
        // default is `u64::default()` = `0`
        pub page: u64,
        /// filters the list on `active.to >= active_to_ge`
        /// It should be the same timestamp format as the `Campaign.active.to`: **seconds**
        #[serde(with = "ts_seconds", default = "Utc::now", rename = "activeTo")]
        pub active_to_ge: DateTime<Utc>,
        pub creator: Option<Address>,
        /// filters the campaigns containing a specific validator if provided
        #[serde(flatten)]
        pub validator: Option<ValidatorParam>,
    }

    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    #[serde(rename_all = "camelCase")]
    pub enum ValidatorParam {
        /// Results will include all campaigns that have the provided address as a leader
        Leader(ValidatorId),
        /// Results will include all campaigns that have either a leader or follower with the provided address
        Validator(ValidatorId),
    }

    #[cfg(test)]
    mod test {
        use super::*;
        use crate::util::tests::prep_db::{ADDRESSES, IDS};
        use chrono::TimeZone;

        #[test]
        pub fn deserialize_campaign_list_query() {
            let query_leader = CampaignListQuery {
                page: 0,
                active_to_ge: Utc.ymd(2021, 2, 1).and_hms(7, 0, 0),
                creator: Some(ADDRESSES["creator"]),
                validator: Some(ValidatorParam::Leader(IDS["leader"])),
            };

            let query_leader_string = format!(
                "page=0&activeTo=1612162800&creator={}&leader={}",
                ADDRESSES["creator"], ADDRESSES["leader"]
            );
            let query_leader_encoded =
                serde_urlencoded::from_str::<CampaignListQuery>(&query_leader_string)
                    .expect("should encode");

            pretty_assertions::assert_eq!(query_leader_encoded, query_leader);

            let query_validator = CampaignListQuery {
                page: 0,
                active_to_ge: Utc.ymd(2021, 2, 1).and_hms(7, 0, 0),
                creator: Some(ADDRESSES["creator"]),
                validator: Some(ValidatorParam::Validator(IDS["follower"])),
            };
            let query_validator_string = format!(
                "page=0&activeTo=1612162800&creator={}&validator={}",
                ADDRESSES["creator"], ADDRESSES["follower"]
            );
            let query_validator_encoded =
                serde_urlencoded::from_str::<CampaignListQuery>(&query_validator_string)
                    .expect("should encode");

            pretty_assertions::assert_eq!(query_validator_encoded, query_validator,);

            let query_no_validator = CampaignListQuery {
                page: 0,
                active_to_ge: Utc.ymd(2021, 2, 1).and_hms(7, 0, 0),
                creator: Some(ADDRESSES["creator"]),
                validator: None,
            };

            let query_no_validator_string = format!(
                "page=0&activeTo=1612162800&creator={}",
                ADDRESSES["creator"]
            );
            let query_no_validator_encoded =
                serde_urlencoded::from_str::<CampaignListQuery>(&query_no_validator_string)
                    .expect("should encode");

            pretty_assertions::assert_eq!(query_no_validator_encoded, query_no_validator,);
        }
    }
}

pub mod campaign_create {
    use chrono::{serde::ts_milliseconds, DateTime, Utc};
    use serde::{Deserialize, Serialize};

    use crate::{
        campaign::{prefix_active, Active, PricingBounds, Validators},
        targeting::Rules,
        AdUnit, Address, Campaign, CampaignId, Channel, EventSubmission, UnifiedNum,
    };

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    #[serde(rename_all = "camelCase")]
    /// All fields are present except the `CampaignId` which is randomly created
    /// This struct defines the Body of the request (in JSON)
    pub struct CreateCampaign {
        pub id: Option<CampaignId>,
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
        /// Creates a new [`Campaign`]
        /// If [`CampaignId`] was not provided with the request it will be generated using [`CampaignId::new()`]
        pub fn into_campaign(self) -> Campaign {
            Campaign {
                id: self.id.unwrap_or_else(CampaignId::new),
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

        /// Creates a [`CreateCampaign`] without using the [`Campaign.id`].
        /// You can either pass [`None`] to randomly generate a new [`CampaignId`].
        /// Or you can pass a [`CampaignId`] to be used for the [`CreateCampaign`].
        pub fn from_campaign_erased(campaign: Campaign, id: Option<CampaignId>) -> Self {
            CreateCampaign {
                id,
                channel: campaign.channel,
                creator: campaign.creator,
                budget: campaign.budget,
                validators: campaign.validators,
                title: campaign.title,
                pricing_bounds: campaign.pricing_bounds,
                event_submission: campaign.event_submission,
                ad_units: campaign.ad_units,
                targeting_rules: campaign.targeting_rules,
                created: campaign.created,
                active: campaign.active,
            }
        }

        /// This function will retains the original [`Campaign.id`] ([`CampaignId`]).
        pub fn from_campaign(campaign: Campaign) -> Self {
            let id = Some(campaign.id);
            Self::from_campaign_erased(campaign, id)
        }
    }

    // /// This implementation helps with test setup
    // /// **NOTE:** It erases the CampaignId, since the creation of the campaign gives it's CampaignId
    // impl From<Campaign> for CreateCampaign {
    //     fn from(campaign: Campaign) -> Self {
    //         Self {
    //             id: Some(campaign.id),
    //             channel: campaign.channel,
    //             creator: campaign.creator,
    //             budget: campaign.budget,
    //             validators: campaign.validators,
    //             title: campaign.title,
    //             pricing_bounds: campaign.pricing_bounds,
    //             event_submission: campaign.event_submission,
    //             ad_units: campaign.ad_units,
    //             targeting_rules: campaign.targeting_rules,
    //             created: campaign.created,
    //             active: campaign.active,
    //         }
    //     }
    // }

    // All editable fields stored in one place, used for checking when a budget is changed
    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    pub struct ModifyCampaign {
        pub budget: Option<UnifiedNum>,
        pub validators: Option<Validators>,
        pub title: Option<String>,
        pub pricing_bounds: Option<PricingBounds>,
        pub event_submission: Option<EventSubmission>,
        pub ad_units: Option<Vec<AdUnit>>,
        pub targeting_rules: Option<Rules>,
    }

    impl ModifyCampaign {
        pub fn from_campaign(campaign: Campaign) -> Self {
            ModifyCampaign {
                budget: Some(campaign.budget),
                validators: Some(campaign.validators),
                title: campaign.title,
                pricing_bounds: campaign.pricing_bounds,
                event_submission: campaign.event_submission,
                ad_units: Some(campaign.ad_units),
                targeting_rules: Some(campaign.targeting_rules),
            }
        }

        pub fn apply(self, mut campaign: Campaign) -> Campaign {
            if let Some(new_budget) = self.budget {
                campaign.budget = new_budget;
            }

            if let Some(new_validators) = self.validators {
                campaign.validators = new_validators;
            }

            // check if it was passed otherwise not sending a Title will result in clearing of the current one
            if let Some(new_title) = self.title {
                campaign.title = Some(new_title);
            }

            if let Some(new_pricing_bounds) = self.pricing_bounds {
                campaign.pricing_bounds = Some(new_pricing_bounds);
            }

            if let Some(new_event_submission) = self.event_submission {
                campaign.event_submission = Some(new_event_submission);
            }

            if let Some(new_ad_units) = self.ad_units {
                campaign.ad_units = new_ad_units;
            }

            if let Some(new_targeting_rules) = self.targeting_rules {
                campaign.targeting_rules = new_targeting_rules;
            }

            campaign
        }
    }
}

#[cfg(feature = "postgres")]
mod postgres {
    use super::{Analytics, MessageResponse, ValidatorMessage};
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

    impl From<&Row> for Analytics {
        fn from(row: &Row) -> Self {
            let ad_slot_type: String = row.get("ad_slot_type");
            let ad_slot_type = match ad_slot_type.len() {
                0 => None,
                _ => Some(ad_slot_type),
            };

            let hostname: String = row.get("hostname");
            let hostname = match hostname.len() {
                0 => None,
                _ => Some(hostname),
            };

            let country: String = row.get("country");
            let country = match country.len() {
                0 => None,
                _ => Some(country),
            };

            Self {
                campaign_id: row.get("campaign_id"),
                time: row.get("time"),
                ad_unit: row.try_get("ad_unit").ok(),
                ad_slot: row.try_get("ad_slot").ok(),
                ad_slot_type,
                advertiser: row.get("advertiser"),
                publisher: row.get("publisher"),
                hostname,
                country,
                os_name: row.get("os"),
                event_type: row.get("event_type"),
                payout_amount: row.get("payout_amount"),
                payout_count: row.get("payout_count"),
            }
        }
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
            ad_unit: Some(DUMMY_IPFS[0]),
            ad_slot: Some(DUMMY_IPFS[1]),
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
