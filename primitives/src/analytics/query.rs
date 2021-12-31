use chrono::Utc;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, fmt};

use crate::sentry::DateHour;

use super::Timeframe;

/// When adding new [`AllowedKey`] make sure to update the [`ALLOWED_KEYS`] static value.
#[derive(Debug, Serialize, Deserialize, Hash, PartialEq, Eq, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub enum AllowedKey {
    CampaignId,
    AdUnit,
    AdSlot,
    AdSlotType,
    Advertiser,
    Publisher,
    Hostname,
    Country,
    OsName,
}

impl fmt::Display for AllowedKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let json_value = serde_json::to_value(self).expect("Should never fail serialization!");
        let string = json_value
            .as_str()
            .expect("Json value should always be String!");

        f.write_str(&string)
    }
}

/// All [`AllowedKey`]s should be present in this static variable.
pub static ALLOWED_KEYS: Lazy<HashSet<AllowedKey>> = Lazy::new(|| {
    vec![
        AllowedKey::CampaignId,
        AllowedKey::AdUnit,
        AllowedKey::AdSlot,
        AllowedKey::AdSlotType,
        AllowedKey::Advertiser,
        AllowedKey::Publisher,
        AllowedKey::Hostname,
        AllowedKey::Country,
        AllowedKey::OsName,
    ]
    .into_iter()
    .collect()
});

#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
pub struct Time {
    // pub struct Time<Tz: chrono::TimeZone> {
    // #[serde(default = "default_timeframe")]
    pub timeframe: Timeframe,
    /// The default value used will be [`DateHour::now`] - [`AnalyticsQuery::timeframe`]
    /// For this query parameter you can use either:
    /// - a string with RFC 3339 and ISO 8601 format (see [`chrono::DateTime::parse_from_rfc3339`])
    /// - a timestamp in milliseconds
    /// **Note:** [`DateHour`] rules should be uphold, this means that passed values should always be rounded to hours
    /// And it should not contain **minutes**, **seconds** or **nanoseconds**
    // TODO: When deserializing AnalyticsQuery, take timeframe & timezone into account and impl Default value
    pub start: DateHour<Utc>,
    // #[serde(default, deserialize_with = "deserialize_query_time")]
    pub end: Option<DateHour<Utc>>,
    // we can use `chrono_tz` to support more Timezones when needed.
    // #[serde(default = "default_timezone_utc")]
    // pub timezone: Tz,//: chrono::TimeZone,
}
mod de {
    use crate::{analytics::Timeframe, sentry::DateHour};

    use super::Time;
    use serde::{
        de::{self, MapAccess, Visitor},
        Deserialize, Deserializer,
    };
    use std::fmt;

    impl<'de> Deserialize<'de> for Time {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            #[derive(Deserialize)]
            #[serde(field_identifier, rename_all = "lowercase")]
            enum Field {
                Timeframe,
                Start,
                End,
            }

            struct TimeVisitor;

            impl<'de> Visitor<'de> for TimeVisitor {
                type Value = Time;

                fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                    formatter.write_str("struct Time")
                }

                fn visit_map<V>(self, mut map: V) -> Result<Time, V::Error>
                where
                    V: MapAccess<'de>,
                {
                    let mut timeframe = None;
                    let mut start = None;
                    let mut end = None;
                    while let Some(key) = map.next_key()? {
                        match key {
                            Field::Timeframe => {
                                if timeframe.is_some() {
                                    return Err(de::Error::duplicate_field("timeframe"));
                                }
                                timeframe = Some(map.next_value()?);
                            }
                            Field::Start => {
                                if start.is_some() {
                                    return Err(de::Error::duplicate_field("start"));
                                }
                                start = Some(map.next_value()?);
                            }
                            Field::End => {
                                if end.is_some() {
                                    return Err(de::Error::duplicate_field("end"));
                                }
                                end = Some(map.next_value()?);
                            }
                        }
                    }

                    let timeframe = timeframe.unwrap_or(Timeframe::Day);
                    let start = start.unwrap_or_else(|| DateHour::now() - &timeframe);
                    Ok(Time {
                        timeframe,
                        start,
                        end,
                    })
                }
            }

            const FIELDS: &'static [&'static str] = &["timeframe", "start", "end"];
            deserializer.deserialize_struct("Time", FIELDS, TimeVisitor)
        }
    }
}

#[cfg(test)]
mod test {
    use serde_json::{from_value, json};

    use crate::{analytics::Timeframe, sentry::DateHour};

    use super::Time;

    #[test]
    fn deserialize_time() {
        // default values for empty JSON object
        {
            let empty = json!({});

            let time = from_value::<Time>(empty).expect("Should use defaults on empty JSON");
            pretty_assertions::assert_eq!(
                time,
                Time {
                    timeframe: Timeframe::Day,
                    start: DateHour::now() - &Timeframe::Day,
                    end: None
                }
            );
        }

        // default values for some fields
        {}
    }
}
