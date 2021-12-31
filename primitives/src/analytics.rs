use crate::{sentry::DateHour, Address, CampaignId, ValidatorId, IPFS};
use chrono::{serde::ts_milliseconds_option, Utc};
use parse_display::Display;
use serde::{Deserialize, Deserializer, Serialize};

use self::query::AllowedKey;

pub const ANALYTICS_QUERY_LIMIT: u32 = 200;

#[cfg(feature = "postgres")]
pub mod postgres {
    use super::{query::AllowedKey, AnalyticsQuery, OperatingSystem};
    use bytes::BytesMut;
    use std::error::Error;
    use tokio_postgres::types::{accepts, to_sql_checked, FromSql, IsNull, ToSql, Type};

    impl AnalyticsQuery {
        pub fn get_key(&self, key: AllowedKey) -> Option<Box<dyn ToSql + Sync + Send>> {
            match key {
                AllowedKey::CampaignId => self.campaign_id.map(Into::into),
                AllowedKey::AdUnit => self.ad_unit.map(Into::into),
                AllowedKey::AdSlot => self.ad_slot.map(Into::into),
                AllowedKey::AdSlotType => self.ad_slot_type.clone().map(Into::into),
                AllowedKey::Advertiser => self.advertiser.map(Into::into),
                AllowedKey::Publisher => self.publisher.map(Into::into),
                AllowedKey::Hostname => self.hostname.clone().map(Into::into),
                AllowedKey::Country => self.country.clone().map(Into::into),
                AllowedKey::OsName => self.os_name.clone().map(Into::into),
            }
        }
    }

    impl<'a> FromSql<'a> for OperatingSystem {
        fn from_sql(ty: &Type, raw: &'a [u8]) -> Result<Self, Box<dyn Error + Sync + Send>> {
            let str_slice = <&str as FromSql>::from_sql(ty, raw)?;
            let os = match str_slice {
                "Other" => OperatingSystem::Other,
                "Linux" => OperatingSystem::Linux,
                _ => OperatingSystem::Whitelisted(str_slice.to_string()),
            };

            Ok(os)
        }

        accepts!(TEXT, VARCHAR);
    }

    impl ToSql for OperatingSystem {
        fn to_sql(
            &self,
            ty: &Type,
            w: &mut BytesMut,
        ) -> Result<IsNull, Box<dyn Error + Sync + Send>> {
            self.to_string().to_sql(ty, w)
        }

        accepts!(TEXT, VARCHAR);
        to_sql_checked!();
    }

    impl ToSql for AllowedKey {
        fn to_sql(
            &self,
            ty: &Type,
            w: &mut BytesMut,
        ) -> Result<IsNull, Box<dyn Error + Sync + Send>> {
            self.to_string().to_sql(ty, w)
        }

        accepts!(TEXT, VARCHAR);
        to_sql_checked!();
    }

    impl<'a> FromSql<'a> for AllowedKey {
        fn from_sql(ty: &Type, raw: &'a [u8]) -> Result<Self, Box<dyn Error + Sync + Send>> {
            let allowed_key_string = String::from_sql(ty, raw)?;

            let allowed_key =
                serde_json::from_value(serde_json::Value::String(allowed_key_string))?;

            Ok(allowed_key)
        }

        accepts!(TEXT, VARCHAR);
    }
}

pub mod query;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AnalyticsQuery {
    #[serde(default = "default_limit")]
    pub limit: u32,
    #[serde(default = "default_event_type")]
    pub event_type: String,
    #[serde(default = "default_metric")]
    pub metric: Metric,
    #[serde(default = "default_timeframe")]
    pub timeframe: Timeframe,
    pub segment_by: Option<AllowedKey>,
    /// The default value used will be [`DateHour::now`] - [`AnalyticsQuery::timeframe`]
    /// For this query parameter you can use either:
    /// - a string with RFC 3339 and ISO 8601 format (see [`chrono::DateTime::parse_from_rfc3339`])
    /// - a timestamp in milliseconds
    /// **Note:** [`DateHour`] rules should be uphold, this means that passed values should always be rounded to hours
    /// the should not contain **minutes**, **seconds** or **nanoseconds**
    // TODO: When deserializing AnalyticsQuery, take timeframe & timezone into account and impl Default value
    // #[serde(default, deserialize_with = "deserialize_query_time")]
    pub start: Option<DateHour<Utc>>,
    // #[serde(default, deserialize_with = "deserialize_query_time")]
    pub end: Option<DateHour<Utc>>,
    // #[serde(flatten)]
    // pub time: Time,
    // #[serde(default = "default_timezone")]
    // pub timezone: String,
    pub campaign_id: Option<CampaignId>,
    pub ad_unit: Option<IPFS>,
    pub ad_slot: Option<IPFS>,
    pub ad_slot_type: Option<String>,
    pub advertiser: Option<Address>,
    pub publisher: Option<Address>,
    pub hostname: Option<String>,
    pub country: Option<String>,
    pub os_name: Option<OperatingSystem>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged, rename_all = "camelCase")]
pub enum AnalyticsQueryKey {
    CampaignId(CampaignId),
    IPFS(IPFS),
    String(String),
    Address(Address),
    OperatingSystem(OperatingSystem),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Display, Hash, Eq)]
#[serde(untagged, into = "String", from = "String")]
pub enum OperatingSystem {
    Linux,
    #[display("{0}")]
    Whitelisted(String),
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize, Display, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Timeframe {
    Year,
    Month,
    Week,
    Day,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Display)]
#[serde(rename_all = "camelCase")]
pub enum Metric {
    Count,
    Paid,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Display)]
pub enum AuthenticateAs {
    #[display("{0}")]
    Advertiser(ValidatorId),
    #[display("{0}")]
    Publisher(ValidatorId),
}

// TODO: Move the postgres module
impl Metric {
    pub fn column_name(self) -> String {
        match self {
            Metric::Count => "payout_count".to_string(),
            Metric::Paid => "payout_amount".to_string(),
        }
    }
}

impl Timeframe {
    pub fn to_hours(&self) -> i64 {
        let hour = 1;
        let day = 24 * hour;
        let year = 365 * day;
        match self {
            Timeframe::Day => day,
            Timeframe::Week => 7 * day,
            Timeframe::Month => year / 12,
            Timeframe::Year => year,
        }
    }
}

impl Default for OperatingSystem {
    fn default() -> Self {
        Self::Other
    }
}

impl From<String> for OperatingSystem {
    fn from(operating_system: String) -> Self {
        match operating_system.as_str() {
            "Linux" => OperatingSystem::Linux,
            "Other" => OperatingSystem::Other,
            _ => OperatingSystem::Whitelisted(operating_system),
        }
    }
}

impl From<OperatingSystem> for String {
    fn from(os: OperatingSystem) -> String {
        os.to_string()
    }
}

impl OperatingSystem {
    pub const LINUX_DISTROS: [&'static str; 17] = [
        "Arch",
        "CentOS",
        "Slackware",
        "Fedora",
        "Debian",
        "Deepin",
        "elementary OS",
        "Gentoo",
        "Mandriva",
        "Manjaro",
        "Mint",
        "PCLinuxOS",
        "Raspbian",
        "Sabayon",
        "SUSE",
        "Ubuntu",
        "RedHat",
    ];
    pub const WHITELISTED: [&'static str; 18] = [
        "Android",
        "Android-x86",
        "iOS",
        "BlackBerry",
        "Chromium OS",
        "Fuchsia",
        "Mac OS",
        "Windows",
        "Windows Phone",
        "Windows Mobile",
        "Linux",
        "NetBSD",
        "Nintendo",
        "OpenBSD",
        "PlayStation",
        "Tizen",
        "Symbian",
        "KAIOS",
    ];

    pub fn map_os(os_name: &str) -> OperatingSystem {
        if OperatingSystem::LINUX_DISTROS
            .iter()
            .any(|distro| os_name.eq(*distro))
        {
            OperatingSystem::Linux
        } else if OperatingSystem::WHITELISTED
            .iter()
            .any(|whitelisted| os_name.eq(*whitelisted))
        {
            OperatingSystem::Whitelisted(os_name.into())
        } else {
            OperatingSystem::Other
        }
    }
}

fn default_limit() -> u32 {
    100
}

fn default_event_type() -> String {
    "IMPRESSION".into()
}

fn default_metric() -> Metric {
    Metric::Count
}

fn default_timeframe() -> Timeframe {
    Timeframe::Day
}

fn deserialize_query_time<'de, D>(deserializer: D) -> Result<Option<DateHour<Utc>>, D::Error>
where
    D: Deserializer<'de>,
{
    // let date_as_str = match Option::<&str>::deserialize(deserializer)? {
    //     Some(value) => value,
    //     // return early with None
    //     None => return Ok(None),
    // };

    let datehour = match ts_milliseconds_option::deserialize(deserializer) {
        Ok(Some(datetime)) => DateHour::try_from(datetime).map_err(serde::de::Error::custom)?,
        // return early with None
        Ok(None) => return Ok(None),
        // if we have an error trying to parse the value as milliseconds
        // try to deserialize from string
        Err(_err) => todo!(),
        // match Option::<&str>::deserialize(deserializer)? {
        //     Some(value) => {
        //         let datetime = DateTime::parse_from_rfc3339(value)
        //             .map(|fixed| DateTime::<Utc>::from(fixed))
        //             .map_err(serde::de::Error::custom)?;

        //         DateHour::try_from(datetime).map_err(serde::de::Error::custom)?
        //     }
        //     // return early with None
        //     None => return Ok(None),
        // },
    };

    Ok(Some(datehour))
}

// fn default_timezone() -> String {
//     "UTC".into()
// }

#[cfg(test)]
mod test {
    use super::*;

    #[cfg(feature = "postgres")]
    use crate::postgres::POSTGRES_POOL;
    use once_cell::sync::Lazy;
    use serde_json::{from_value, to_value, Value};
    use std::collections::HashMap;

    static TEST_CASES: Lazy<HashMap<String, (OperatingSystem, Value)>> = Lazy::new(|| {
        vec![
            // Whitelisted - Android
            (
                OperatingSystem::WHITELISTED[0].to_string(),
                (
                    OperatingSystem::Whitelisted("Android".into()),
                    Value::String("Android".into()),
                ),
            ),
            // Linux - Arch
            (
                OperatingSystem::LINUX_DISTROS[0].to_string(),
                (OperatingSystem::Linux, Value::String("Linux".into())),
            ),
            // Other - OS xxxxx
            (
                "OS xxxxx".into(),
                (OperatingSystem::Other, Value::String("Other".into())),
            ),
        ]
        .into_iter()
        .collect()
    });

    #[cfg(feature = "postgres")]
    #[tokio::test]
    async fn os_to_from_sql() {
        let client = POSTGRES_POOL.get().await.unwrap();
        let sql_type = "VARCHAR";

        for (input, _) in TEST_CASES.iter() {
            let actual_os = OperatingSystem::map_os(input);

            // from SQL
            {
                let row_os: OperatingSystem = client
                    .query_one(&*format!("SELECT '{}'::{}", actual_os, sql_type), &[])
                    .await
                    .unwrap()
                    .get(0);

                assert_eq!(
                    &actual_os, &row_os,
                    "expected and actual FromSql differ for {}",
                    input
                );
            }

            // to SQL
            {
                let row_os: OperatingSystem = client
                    .query_one(&*format!("SELECT $1::{}", sql_type), &[&actual_os])
                    .await
                    .unwrap()
                    .get(0);
                assert_eq!(
                    &actual_os, &row_os,
                    "expected and actual ToSql differ for {}",
                    input
                );
            }
        }
    }

    #[test]
    fn test_operating_system() {
        for (input, (expect_os, expect_json)) in TEST_CASES.iter() {
            let actual_os = OperatingSystem::map_os(input);

            assert_eq!(
                expect_os, &actual_os,
                "expected and actual differ for {}",
                input
            );

            let actual_json = to_value(&actual_os).expect("Should serialize it");

            assert_eq!(expect_json, &actual_json);

            let from_json: OperatingSystem =
                from_value(actual_json).expect("Should deserialize it");

            assert_eq!(expect_os, &from_json, "error processing {}", input);
        }
    }
}
