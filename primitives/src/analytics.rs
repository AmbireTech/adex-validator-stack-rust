use crate::{sentry::DateHour, Address, CampaignId, ValidatorId, IPFS};
use chrono::Utc;
use parse_display::Display;
use serde::{Deserialize, Serialize};

pub const ANALYTICS_QUERY_LIMIT: u32 = 200;

#[cfg(feature = "postgres")]
pub mod postgres {
    use super::{AnalyticsQueryKey, Metric, OperatingSystem};
    use bytes::BytesMut;
    use std::error::Error;
    use tokio_postgres::types::{accepts, to_sql_checked, FromSql, IsNull, ToSql, Type};

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

    impl ToSql for AnalyticsQueryKey {
        fn to_sql(
            &self,
            ty: &Type,
            w: &mut BytesMut,
        ) -> Result<IsNull, Box<dyn Error + Sync + Send>> {
            match self {
                Self::CampaignId(id) => id.to_string().to_sql(ty, w),
                Self::AdUnit(ipfs) | Self::AdSlot(ipfs) => ipfs.to_string().to_sql(ty, w),
                Self::AdSlotType(value) | Self::Hostname(value) | Self::Country(value) => {
                    value.to_sql(ty, w)
                }
                Self::Advertiser(addr) | Self::Publisher(addr) => addr.to_string().to_sql(ty, w),
                Self::OperatingSystem(os_name) => os_name.to_string().to_sql(ty, w),
            }
        }

        accepts!(TEXT, VARCHAR);
        to_sql_checked!();
    }

    impl ToSql for Metric {
        fn to_sql(
            &self,
            ty: &Type,
            w: &mut BytesMut,
        ) -> Result<IsNull, Box<dyn Error + Sync + Send>> {
            self.column_name().to_sql(ty, w)
        }

        accepts!(TEXT, VARCHAR);
        to_sql_checked!();
    }
}

#[derive(Debug, Serialize, Deserialize)]
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
    pub segment_by: Option<String>,
    pub start: Option<DateHour<Utc>>,
    pub end: Option<DateHour<Utc>>,
    // #[serde(default = "default_timezone")]
    // pub timezone: String,
    #[serde(flatten)]
    pub campaign_id: Option<AnalyticsQueryKey>,
    #[serde(flatten)]
    pub ad_unit: Option<AnalyticsQueryKey>,
    #[serde(flatten)]
    pub ad_slot: Option<AnalyticsQueryKey>,
    #[serde(flatten)]
    pub ad_slot_type: Option<AnalyticsQueryKey>,
    #[serde(flatten)]
    pub advertiser: Option<AnalyticsQueryKey>,
    #[serde(flatten)]
    pub publisher: Option<AnalyticsQueryKey>,
    #[serde(flatten)]
    pub hostname: Option<AnalyticsQueryKey>,
    #[serde(flatten)]
    pub country: Option<AnalyticsQueryKey>,
    #[serde(flatten)]
    pub os_name: Option<AnalyticsQueryKey>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AnalyticsQueryKey {
    CampaignId(CampaignId),
    AdUnit(IPFS),
    AdSlot(IPFS),
    AdSlotType(String),
    Advertiser(Address),
    Publisher(Address),
    Hostname(String),
    Country(String),
    OperatingSystem(OperatingSystem),
}

impl AnalyticsQuery {
    pub fn get_key(&self, key: &str) -> &Option<AnalyticsQueryKey> {
        match key {
            "campaignId" => &self.campaign_id,
            "adUnit" => &self.ad_unit,
            "adSlot" => &self.ad_slot,
            "adSlotType" => &self.ad_slot_type,
            "advertiser" => &self.advertiser,
            "publisher" => &self.publisher,
            "hostname" => &self.hostname,
            "country" => &self.country,
            "osName" => &self.os_name,
            _ => &None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Display, Hash, Eq)]
#[serde(untagged, into = "String", from = "String")]
pub enum OperatingSystem {
    Linux,
    #[display("{0}")]
    Whitelisted(String),
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize, Display)]
#[serde(rename_all = "camelCase")]
pub enum Timeframe {
    Year,
    Month,
    Week,
    Day,
}

#[derive(Debug, Clone, Serialize, Deserialize, Display)]
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

impl AuthenticateAs {
    pub fn try_from(key: &str, uid: ValidatorId) -> Option<Self> {
        match key {
            "advertiser" => Some(Self::Advertiser(uid)),
            "publisher" => Some(Self::Publisher(uid)),
            // TODO: Should we throw an error here
            _ => None,
        }
    }
}

impl Metric {
    pub fn column_name(&self) -> String {
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
