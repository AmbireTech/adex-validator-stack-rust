use crate::{
    analytics::{OperatingSystem, Timeframe},
    balances::BalancesState,
    spender::Spender,
    validator::{ApproveState, Heartbeat, MessageTypes, NewState, Type as MessageType},
    Address, Balances, BigNum, CampaignId, Channel, ChannelId, UnifiedNum, ValidatorId, IPFS,
};
use chrono::{
    serde::ts_milliseconds, Date, DateTime, Duration, NaiveDate, TimeZone, Timelike, Utc,
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{
    cmp::{Ord, Ordering},
    collections::HashMap,
    fmt,
    hash::Hash,
    ops::Sub,
};
use thiserror::Error;

pub use event::{Event, EventType, CLICK, IMPRESSION};

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
    use std::ops::Deref;

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

mod event {
    use once_cell::sync::Lazy;
    use parse_display::{Display, FromStr};
    use serde::{Deserialize, Serialize};
    use std::fmt;

    use crate::{Address, IPFS};

    pub static IMPRESSION: EventType = EventType::Impression;
    pub static CLICK: EventType = EventType::Click;

    /// We use these statics to create the `as_str()` method for a value with a `'static` lifetime
    /// the `parse_display::Display` derive macro does not impl such methods
    static IMPRESSION_STRING: Lazy<String> = Lazy::new(|| EventType::Impression.to_string());
    static CLICK_STRING: Lazy<String> = Lazy::new(|| EventType::Click.to_string());

    #[derive(
        Debug,
        Display,
        FromStr,
        Serialize,
        Deserialize,
        Hash,
        Ord,
        Eq,
        PartialEq,
        PartialOrd,
        Clone,
        Copy,
    )]
    #[display(style = "SNAKE_CASE")]
    #[serde(rename_all = "SCREAMING_SNAKE_CASE")]
    pub enum EventType {
        Impression,
        Click,
    }

    impl EventType {
        pub fn as_str(&self) -> &str {
            match self {
                EventType::Impression => IMPRESSION_STRING.as_str(),
                EventType::Click => CLICK_STRING.as_str(),
            }
        }
    }

    #[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Hash)]
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

        pub fn event_type(&self) -> EventType {
            self.into()
        }
    }

    impl From<&Event> for EventType {
        fn from(event: &Event) -> Self {
            match event {
                Event::Impression { .. } => EventType::Impression,
                Event::Click { .. } => EventType::Click,
            }
        }
    }

    impl AsRef<str> for Event {
        fn as_ref(&self) -> &'static str {
            match self {
                Event::Impression { .. } => EventType::Impression.as_str(),
                Event::Click { .. } => EventType::Click.as_str(),
            }
        }
    }

    impl fmt::Display for Event {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str(self.as_ref())
        }
    }

    #[cfg(test)]
    mod test {
        use crate::sentry::event::{CLICK_STRING, IMPRESSION_STRING};

        use super::EventType;

        #[test]
        fn event_type_parsing_and_de_serialization() {
            let impression_parse = "IMPRESSION"
                .parse::<EventType>()
                .expect("Should parse IMPRESSION");
            let click_parse = "CLICK".parse::<EventType>().expect("Should parse CLICK");
            let impression_json =
                serde_json::from_value::<EventType>(serde_json::Value::String("IMPRESSION".into()))
                    .expect("Should deserialize");
            let click_json =
                serde_json::from_value::<EventType>(serde_json::Value::String("CLICK".into()))
                    .expect("Should deserialize");

            assert_eq!(IMPRESSION_STRING.as_str(), "IMPRESSION");
            assert_eq!(CLICK_STRING.as_str(), "CLICK");
            assert_eq!(EventType::Impression, impression_parse);
            assert_eq!(EventType::Impression, impression_json);
            assert_eq!(EventType::Click, click_parse);
            assert_eq!(EventType::Click, click_json);
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct UpdateAnalytics {
    pub time: DateHour<Utc>,
    pub campaign_id: CampaignId,
    pub ad_unit: Option<IPFS>,
    pub ad_slot: Option<IPFS>,
    pub ad_slot_type: Option<String>,
    pub advertiser: Address,
    pub publisher: Address,
    pub hostname: Option<String>,
    pub country: Option<String>,
    pub os_name: OperatingSystem,
    pub event_type: EventType,
    pub amount_to_add: UnifiedNum,
    pub count_to_add: i32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Analytics {
    pub time: DateHour<Utc>,
    pub campaign_id: CampaignId,
    pub ad_unit: Option<IPFS>,
    pub ad_slot: Option<IPFS>,
    pub ad_slot_type: Option<String>,
    pub advertiser: Address,
    pub publisher: Address,
    pub hostname: Option<String>,
    pub country: Option<String>,
    pub os_name: OperatingSystem,
    pub event_type: EventType,
    pub payout_amount: UnifiedNum,
    pub payout_count: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct FetchedAnalytics {
    // time is represented as a timestamp
    #[serde(with = "ts_milliseconds")]
    pub time: DateTime<Utc>,
    pub value: FetchedMetric,
    // We can't know the exact segment type but it can always be represented as a string
    pub segment: Option<String>,
}

/// The value of the requested analytics [`Metric`].
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(untagged)]
pub enum FetchedMetric {
    Count(u32),
    Paid(UnifiedNum),
}

impl FetchedMetric {
    /// Returns the count if it's a [`FetchedMetric::Count`] or `None` otherwise.
    pub fn get_count(&self) -> Option<u32> {
        match self {
            FetchedMetric::Count(count) => Some(*count),
            FetchedMetric::Paid(_) => None,
        }
    }

    /// Returns the paid amount if it's a [`FetchedMetric::Paid`] or `None` otherwise.
    pub fn get_paid(&self) -> Option<UnifiedNum> {
        match self {
            FetchedMetric::Count(_) => None,
            FetchedMetric::Paid(paid) => Some(*paid),
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
#[error("Minutes ({minutes}), seconds ({seconds}) & nanoseconds ({nanoseconds}) should all be set to 0 (zero)")]
pub struct DateHourError {
    pub minutes: u32,
    pub seconds: u32,
    pub nanoseconds: u32,
}

#[derive(Clone, Hash)]
/// [`DateHour`] holds the date and hour (only).
/// It uses [`chrono::DateTime`] when serializing and deserializing.
/// When serializing it always sets minutes and seconds to `0` (zero).
/// When deserializing the minutes and seconds should always be set to `0` (zero),
/// otherwise an error will be returned.
pub struct DateHour<Tz: TimeZone> {
    pub date: Date<Tz>,
    /// hour is in the range of `0 - 23`
    pub hour: u32,
}

impl<Tz: TimeZone> Eq for DateHour<Tz> {}

impl<Tz: TimeZone, Tz2: TimeZone> PartialEq<DateHour<Tz2>> for DateHour<Tz> {
    fn eq(&self, other: &DateHour<Tz2>) -> bool {
        self.date == other.date && self.hour == other.hour
    }
}

impl<Tz: TimeZone> Ord for DateHour<Tz> {
    fn cmp(&self, other: &DateHour<Tz>) -> Ordering {
        match self.date.cmp(&other.date) {
            // Only if the two dates are equal, compare the hours too!
            Ordering::Equal => self.hour.cmp(&other.hour),
            ordering => ordering,
        }
    }
}

impl<Tz: TimeZone, Tz2: TimeZone> PartialOrd<DateHour<Tz2>> for DateHour<Tz> {
    /// Compare two DateHours based on their true time, ignoring time zones
    ///
    /// See [`DateTime`] implementation of `PartialOrd<DateTime<Tz2>>` for more details.
    fn partial_cmp(&self, other: &DateHour<Tz2>) -> Option<Ordering> {
        if self.date == other.date {
            self.hour.partial_cmp(&other.hour)
            // if self.hour > other.hour {
            //     Some(Ordering::Greater)
            // } else if self.hour == other.hour {
            //     Some(Ordering::Equal)
            // } else {
            //     Some(Ordering::Less)
            // }
        } else {
            self.date.naive_utc().partial_cmp(&other.date.naive_utc())
        }
    }
}

impl<Tz: TimeZone> fmt::Display for DateHour<Tz>
where
    Tz::Offset: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let datetime = self.to_datetime();

        datetime.fmt(f)
    }
}

impl DateHour<Utc> {
    /// # Panics
    ///
    /// When wrong inputs have been passed, i.e. for year, month, day or hour.
    pub fn from_ymdh(year: i32, month: u32, day: u32, hour: u32) -> Self {
        Self::from_ymdh_opt(year, month, day, hour).expect("Valid Date with hour")
    }

    /// Makes a new [`DateHour`] from year, month, day and hour.
    ///
    /// Returns `None` on invalid year, month, day or hour.
    ///
    /// See [`chrono::NaiveDate::from_ymd_opt()`] & [`chrono::NaiveTime::from_hms_opt()`] for details
    pub fn from_ymdh_opt(year: i32, month: u32, day: u32, hour: u32) -> Option<Self> {
        if hour >= 24 {
            return None;
        }

        let date = NaiveDate::from_ymd_opt(year, month, day)?;
        Some(Self {
            date: Date::from_utc(date, Utc),
            hour,
        })
    }

    pub fn now() -> Self {
        let datetime = Utc::now();

        Self {
            date: datetime.date(),
            hour: datetime.hour(),
        }
    }
}

/// Manually implement [`Copy`] as it requires a where clause for the [`TimeZone::Offset`]
impl<Tz: TimeZone> Copy for DateHour<Tz> where Tz::Offset: Copy {}

impl<Tz: TimeZone> DateHour<Tz> {
    /// Creates a [`DateTime`] with minutes, seconds, nanoseconds set to `0` (zero)
    pub fn to_datetime(&self) -> DateTime<Tz> {
        self.date.and_hms(self.hour, 0, 0)
    }
}

impl<Tz: TimeZone> fmt::Debug for DateHour<Tz> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.to_datetime().fmt(f)
    }
}

impl<Tz: TimeZone> TryFrom<DateTime<Tz>> for DateHour<Tz> {
    type Error = DateHourError;

    fn try_from(datetime: DateTime<Tz>) -> Result<Self, Self::Error> {
        let time = datetime.time();

        match (time.minute(), time.second(), time.nanosecond()) {
            (0, 0, 0) => Ok(Self {
                date: datetime.date(),
                hour: datetime.hour(),
            }),
            _ => Err(DateHourError {
                minutes: datetime.minute(),
                seconds: datetime.second(),
                nanoseconds: datetime.nanosecond(),
            }),
        }
    }
}

impl<Tz: TimeZone> Sub<&Timeframe> for DateHour<Tz> {
    type Output = DateHour<Tz>;

    fn sub(self, rhs: &Timeframe) -> Self::Output {
        let result = self.to_datetime() - Duration::hours(rhs.to_hours());

        DateHour {
            date: result.date(),
            hour: result.hour(),
        }
    }
}

/// Subtracts **X** hours from the [`DateHour`]
impl<Tz: TimeZone> Sub<i64> for DateHour<Tz> {
    type Output = DateHour<Tz>;

    fn sub(self, rhs: i64) -> Self::Output {
        let result = self.to_datetime() - Duration::hours(rhs);

        DateHour {
            date: result.date(),
            hour: result.hour(),
        }
    }
}

impl Serialize for DateHour<Utc> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.to_datetime().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for DateHour<Utc> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let datetime = <DateTime<Utc>>::deserialize(deserializer)?;

        Self::try_from(datetime).map_err(serde::de::Error::custom)
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

#[derive(Serialize, Deserialize, Debug, PartialEq, Eq, Clone)]
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
    use super::{
        Analytics, DateHour, EventType, FetchedAnalytics, FetchedMetric, MessageResponse,
        ValidatorMessage,
    };
    use crate::{
        analytics::{AnalyticsQuery, Metric},
        sentry::EventAggregate,
        validator::{messages::Type as MessageType, MessageTypes},
    };
    use bytes::BytesMut;
    use chrono::{DateTime, Timelike, Utc};
    use serde::Deserialize;
    use tokio_postgres::{
        types::{accepts, to_sql_checked, FromSql, IsNull, Json, ToSql, Type},
        Error, Row,
    };

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
        /// # Panics
        ///
        /// When a field is missing in [`Row`] or if the [`Analytics`] `ad_unit` or `ad_slot` [`crate::IPFS`] is wrong.
        fn from(row: &Row) -> Self {
            let ad_slot_type = row
                .get::<_, Option<String>>("ad_slot_type")
                .filter(|string| !string.is_empty());
            let hostname = row
                .get::<_, Option<String>>("hostname")
                .filter(|string| !string.is_empty());
            let country = row
                .get::<_, Option<String>>("country")
                .filter(|string| !string.is_empty());

            let ad_unit = row.get::<_, Option<String>>("ad_unit").and_then(|string| {
                if !string.is_empty() {
                    Some(string.parse().expect("Valid IPFS"))
                } else {
                    None
                }
            });
            let ad_slot = row.get::<_, Option<String>>("ad_slot").and_then(|string| {
                if !string.is_empty() {
                    Some(string.parse().expect("Valid IPFS"))
                } else {
                    None
                }
            });

            Self {
                campaign_id: row.get("campaign_id"),
                time: row.get("time"),
                ad_unit,
                ad_slot,
                ad_slot_type,
                advertiser: row.get("advertiser"),
                publisher: row.get("publisher"),
                hostname,
                country,
                os_name: row.get("os_name"),
                event_type: row.get("event_type"),
                payout_amount: row.get("payout_amount"),
                payout_count: row.get::<_, i32>("payout_count").unsigned_abs(),
            }
        }
    }

    impl<'a> FromSql<'a> for DateHour<Utc> {
        fn from_sql(
            ty: &Type,
            raw: &'a [u8],
        ) -> Result<Self, Box<dyn std::error::Error + Sync + Send>> {
            let datetime = <DateTime<Utc> as FromSql>::from_sql(ty, raw)?;
            assert_eq!(datetime.time().minute(), 0);
            assert_eq!(datetime.time().second(), 0);
            assert_eq!(datetime.time().nanosecond(), 0);

            Ok(Self {
                date: datetime.date(),
                hour: datetime.hour(),
            })
        }
        accepts!(TIMESTAMPTZ);
    }

    impl ToSql for DateHour<Utc> {
        fn to_sql(
            &self,
            ty: &Type,
            w: &mut BytesMut,
        ) -> Result<IsNull, Box<dyn std::error::Error + Sync + Send>> {
            self.date.and_hms(self.hour, 0, 0).to_sql(ty, w)
        }

        accepts!(TIMESTAMPTZ);
        to_sql_checked!();
    }

    impl<'a> FromSql<'a> for EventType {
        fn from_sql(
            ty: &Type,
            raw: &'a [u8],
        ) -> Result<Self, Box<dyn std::error::Error + Sync + Send>> {
            let event_string = <&str as FromSql>::from_sql(ty, raw)?;

            Ok(event_string.parse()?)
        }
        accepts!(VARCHAR, TEXT);
    }

    impl ToSql for EventType {
        fn to_sql(
            &self,
            ty: &Type,
            w: &mut BytesMut,
        ) -> Result<IsNull, Box<dyn std::error::Error + Sync + Send>> {
            self.as_str().to_sql(ty, w)
        }

        accepts!(VARCHAR, TEXT);
        to_sql_checked!();
    }

    /// This implementation handles the conversion of a fetched query [`Row`] to [`FetchedAnalytics`]
    /// [`FetchedAnalytics`] requires additional context, apart from [`Row`], using the [`AnalyticsQuery`].
    impl From<(&AnalyticsQuery, &Row)> for FetchedAnalytics {
        /// # Panics
        ///
        /// When a field is missing in the [`Row`].
        fn from((query, row): (&AnalyticsQuery, &Row)) -> Self {
            // Since segment_by is a dynamic value/type it can't be passed to from<&Row> so we're building the object here
            let segment_value = match query.segment_by.as_ref() {
                Some(_segment_by) => row.get("segment_by"),
                None => None,
            };
            let time = row.get::<_, DateTime<Utc>>("timeframe_time");
            let value = match &query.metric {
                Metric::Paid => FetchedMetric::Paid(row.get("value")),
                Metric::Count => {
                    // `integer` fields map to `i32`
                    let count: i32 = row.get("value");
                    // Count can only be positive, so use unsigned value
                    FetchedMetric::Count(count.unsigned_abs())
                }
            };
            FetchedAnalytics {
                time,
                value,
                segment: segment_value,
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::util::tests::prep_db::{ADDRESSES, DUMMY_IPFS};
    use serde_json::{json, Value};

    #[test]
    pub fn test_de_serialize_events() {
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

    #[test]
    fn test_datehour_subtract_timeframe() {
        // test with End of year
        {
            let datehour = DateHour::from_ymdh(2021, 12, 31, 22);

            let yesterday = datehour - &Timeframe::Day;
            let last_week = datehour - &Timeframe::Week;
            let beginning_of_month = datehour - &Timeframe::Month;
            let last_year = datehour - &Timeframe::Year;

            pretty_assertions::assert_eq!(DateHour::from_ymdh(2021, 12, 30, 22), yesterday);
            pretty_assertions::assert_eq!(DateHour::from_ymdh(2021, 12, 24, 22), last_week);
            // Subtracting uses hours so result has different Hour!
            pretty_assertions::assert_eq!(DateHour::from_ymdh(2021, 12, 1, 12), beginning_of_month);
            pretty_assertions::assert_eq!(DateHour::from_ymdh(2020, 12, 31, 22), last_year);
        }

        let middle_of_month = DateHour::from_ymdh(2021, 12, 14, 12) - &Timeframe::Month;
        // Subtracting uses hours so result has different Hour!
        pretty_assertions::assert_eq!(DateHour::from_ymdh(2021, 11, 14, 2), middle_of_month);
    }

    #[test]
    fn test_datehour_de_serialize_partial_ord_and_eq() {
        let earlier = DateHour::from_ymdh(2021, 12, 1, 16);
        let later = DateHour::from_ymdh(2021, 12, 31, 16);

        // Partial Eq
        assert!(earlier < later);
        assert!(!earlier.eq(&later));
        assert!(earlier.eq(&earlier));

        // Partial Ord
        assert_eq!(Some(Ordering::Less), earlier.partial_cmp(&later));
        assert_eq!(Some(Ordering::Greater), later.partial_cmp(&earlier));

        // Serialize & deserialize
        let json_datetime = Value::String("2021-12-01T16:00:00+02:00".into());
        let datehour: DateHour<Utc> =
            serde_json::from_value(json_datetime.clone()).expect("Should deserialize");
        assert_eq!(
            DateHour::from_ymdh(2021, 12, 1, 14),
            datehour,
            "Deserialized DateHour should be 2 hours earlier to match UTC +0"
        );

        let serialized_value = serde_json::to_value(datehour).expect("Should serialize DateHour");
        assert_eq!(
            Value::String("2021-12-01T14:00:00Z".into()),
            serialized_value,
            "Json value should always be serialized with Z (UTC+0) timezone"
        );
    }

    #[test]
    fn test_partial_eq_with_same_datehour_but_different_zones() {
        // Eq & PartialEq with different timezones
        let json = json!({
            "UTC+0": "2021-12-16T14:00:00+00:00",
            "UTC+2": "2021-12-16T16:00:00+02:00",
        });

        let map: HashMap<String, DateHour<Utc>> =
            serde_json::from_value(json).expect("Should deserialize");

        assert_eq!(
            Some(Ordering::Equal),
            map["UTC+0"].partial_cmp(&map["UTC+2"])
        );

        assert_eq!(
            map["UTC+0"], map["UTC+2"],
            "DateHour should be the same after the second one is made into UTC+0"
        );
        assert!(
            map["UTC+0"] >= map["UTC+2"],
            "UTC+0 value should be equal to UTC+2"
        );
    }
}
