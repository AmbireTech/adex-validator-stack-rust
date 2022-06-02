use crate::{
    sentry::{EventType, IMPRESSION},
    Address, CampaignId, ChainId, ValidatorId, IPFS,
};
use parse_display::Display;
use serde::{Deserialize, Serialize};

use self::query::{AllowedKey, Time};

#[cfg(feature = "postgres")]
pub mod postgres {
    use super::{query::AllowedKey, AnalyticsQuery, OperatingSystem};
    use bytes::BytesMut;
    use std::error::Error;
    use tokio_postgres::types::{accepts, to_sql_checked, FromSql, IsNull, ToSql, Type};

    impl AnalyticsQuery {
        pub fn get_key(&self, key: AllowedKey) -> Option<Box<dyn ToSql + Sync + Send>> {
            match key {
                AllowedKey::CampaignId => self
                    .campaign_id
                    .map(|campaign_id| Box::new(campaign_id) as _),
                AllowedKey::AdUnit => self.ad_unit.map(|ad_unit| Box::new(ad_unit) as _),
                AllowedKey::AdSlot => self.ad_slot.map(|ad_slot| Box::new(ad_slot) as _),
                AllowedKey::AdSlotType => self
                    .ad_slot_type
                    .clone()
                    .map(|ad_slot_type| Box::new(ad_slot_type) as _),
                AllowedKey::Advertiser => {
                    self.advertiser.map(|advertiser| Box::new(advertiser) as _)
                }
                AllowedKey::Publisher => self.publisher.map(|publisher| Box::new(publisher) as _),
                AllowedKey::Hostname => self
                    .hostname
                    .clone()
                    .map(|hostname| Box::new(hostname) as _),
                AllowedKey::Country => self.country.clone().map(|country| Box::new(country) as _),
                AllowedKey::OsName => self.os_name.clone().map(|os_name| Box::new(os_name) as _),
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
            let allowed_key_str = <&'a str as FromSql>::from_sql(ty, raw)?;

            Ok(allowed_key_str.parse()?)
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
    pub event_type: EventType,
    #[serde(default = "default_metric")]
    pub metric: Metric,
    pub segment_by: Option<AllowedKey>,
    #[serde(flatten)]
    pub time: Time,
    pub campaign_id: Option<CampaignId>,
    pub ad_unit: Option<IPFS>,
    pub ad_slot: Option<IPFS>,
    pub ad_slot_type: Option<String>,
    pub advertiser: Option<Address>,
    pub publisher: Option<Address>,
    pub hostname: Option<String>,
    pub country: Option<String>,
    pub os_name: Option<OperatingSystem>,
    pub chains: Option<Vec<ChainId>>,
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
    /// [`Timeframe::Year`] returns analytics grouped by month.
    Year,
    /// [`Timeframe::Month`] returns analytics grouped by day.
    Month,
    /// [`Timeframe::Week`] returns analytics grouped by hour.
    /// Same as [`Timeframe::Day`].
    Week,
    /// [`Timeframe::Day`] returns analytics grouped by hour.
    /// Same as [`Timeframe::Week`].
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

impl Metric {
    #[cfg(feature = "postgres")]
    /// Returns the query column name of the [`Metric`].
    ///
    /// Available only when the `postgres` feature is enabled.
    pub fn column_name(self) -> &'static str {
        match self {
            Metric::Count => "payout_count",
            Metric::Paid => "payout_amount",
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

fn default_event_type() -> EventType {
    IMPRESSION
}

fn default_metric() -> Metric {
    Metric::Count
}

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
