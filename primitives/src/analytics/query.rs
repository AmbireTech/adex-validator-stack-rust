use chrono::Utc;
use once_cell::sync::Lazy;
use parse_display::{Display, FromStr};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::sentry::DateHour;

use super::Timeframe;

/// When adding new [`AllowedKey`] make sure to update the [`ALLOWED_KEYS`] static value.
/// When (De)Serializing we use `camelCase`,
/// however, when displaying and parsing the value, we use `snake_case`.
/// The later is particular useful when using the value as column in SQL.
#[derive(Debug, Serialize, Deserialize, Hash, PartialEq, Eq, Clone, Copy, Display, FromStr)]
#[serde(rename_all = "camelCase")]
#[display(style = "snake_case")]
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

impl AllowedKey {
    #[allow(non_snake_case)]
    /// Helper function to get the [`AllowedKey`] as `camelCase`.
    pub fn to_camelCase(&self) -> String {
        serde_json::to_value(self)
            .expect("AllowedKey should always be serializable!")
            .as_str()
            .expect("Serialized AllowedKey should be a string!")
            .to_string()
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

// fn deserialize_query_time<'de, D>(deserializer: D) -> Result<Option<DateHour<Utc>>, D::Error>
// where
//     D: Deserializer<'de>,
// {
//     // let date_as_str = match Option::<&str>::deserialize(deserializer)? {
//     //     Some(value) => value,
//     //     // return early with None
//     //     None => return Ok(None),
//     // };

//     let datehour = match ts_milliseconds_option::deserialize(deserializer) {
//         Ok(Some(datetime)) => DateHour::try_from(datetime).map_err(serde::de::Error::custom)?,
//         // return early with None
//         Ok(None) => return Ok(None),
//         // if we have an error trying to parse the value as milliseconds
//         // try to deserialize from string
//         Err(_err) => todo!(),
//         // match Option::<&str>::deserialize(deserializer)? {
//         //     Some(value) => {
//         //         let datetime = DateTime::parse_from_rfc3339(value)
//         //             .map(|fixed| DateTime::<Utc>::from(fixed))
//         //             .map_err(serde::de::Error::custom)?;

//         //         DateHour::try_from(datetime).map_err(serde::de::Error::custom)?
//         //     }
//         //     // return early with None
//         //     None => return Ok(None),
//         // },
//     };

//     Ok(Some(datehour))
// }

#[derive(Debug, Serialize, Clone, PartialEq, Eq)]
pub struct Time {
    /// Default: [`Timeframe::Day`].
    pub timeframe: Timeframe,
    /// Default value: [`DateHour::now`] - `self.timeframe`
    /// For this query parameter you can use either:
    /// - a string with RFC 3339 and ISO 8601 format (see [`chrono::DateTime::parse_from_rfc3339`])
    /// - a timestamp in milliseconds
    /// **Note:** [`DateHour`] rules should be uphold, this means that passed values should always be rounded to hours
    /// And it should not contain **minutes**, **seconds** or **nanoseconds**
    pub start: DateHour<Utc>,
    /// The End [`DateHour`] which will fetch `analytics_time <= end` and should be after Start [`DateHour`]!
    pub end: Option<DateHour<Utc>>,
    // we can use `chrono_tz` to support more Timezones when needed.
    // #[serde(default = "default_timezone_utc")]
    // pub timezone: Tz,//: chrono::TimeZone,
}

impl Default for Time {
    fn default() -> Self {
        let timeframe = Timeframe::Day;
        let start = DateHour::now() - &timeframe;

        Self {
            timeframe,
            start,
            end: None,
        }
    }
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

                    // if there is an End DateHour passed, check if End is > Start
                    match end {
                        Some(end) if start >= end => {
                            return Err(de::Error::custom(
                                "End time should be larger than the Start time",
                            ));
                        }
                        _ => {}
                    }

                    Ok(Time {
                        timeframe,
                        start,
                        end,
                    })
                }
            }

            const FIELDS: &[&str] = &["timeframe", "start", "end"];
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
            let default = Time::default();
            pretty_assertions::assert_eq!(
                time,
                default,
                "Default should generate the same as the default deserialization values!"
            );
            pretty_assertions::assert_eq!(
                time,
                Time {
                    timeframe: Timeframe::Day,
                    start: DateHour::now() - &Timeframe::Day,
                    end: None
                }
            );
        }

        // `Start` default value and no `End`
        {
            let timeframe_only = json!({
                "timeframe": "week",
            });

            let time = from_value::<Time>(timeframe_only).expect("Should use default for start");
            pretty_assertions::assert_eq!(
                time,
                Time {
                    timeframe: Timeframe::Week,
                    start: DateHour::now() - &Timeframe::Week,
                    end: None
                }
            );
        }

        // all fields with same timezone
        {
            let full = json!({
                "timeframe": "day",
                "start": "2021-12-1T16:00:00+02:00",
                "end": "2021-12-31T16:00:00+02:00"
            });

            let time = from_value::<Time>(full).expect("Should use default for start");
            pretty_assertions::assert_eq!(
                time,
                Time {
                    timeframe: Timeframe::Day,
                    start: DateHour::from_ymdh(2021, 12, 1, 14),
                    end: Some(DateHour::from_ymdh(2021, 12, 31, 14)),
                }
            );
        }

        // All fields with different timezones
        {
            let full = json!({
                "timeframe": "day",
                "start": "2021-12-1T16:00:00+00:00",
                "end": "2021-12-31T16:00:00+02:00"
            });

            let time = from_value::<Time>(full).expect("Should deserialize");
            pretty_assertions::assert_eq!(
                time,
                Time {
                    timeframe: Timeframe::Day,
                    start: DateHour::from_ymdh(2021, 12, 1, 16),
                    end: Some(DateHour::from_ymdh(2021, 12, 31, 14)),
                }
            );
        }

        // Start > End
        {
            let full = json!({
                "timeframe": "day",
                "start": "2021-12-16T16:00:00+00:00",
                "end": "2021-12-15T16:00:00+02:00"
            });

            let err = from_value::<Time>(full).expect_err("Should error because Start > End");

            assert_eq!(
                "End time should be larger than the Start time",
                err.to_string()
            );
        }

        // Start = End with different timezones
        {
            let full = json!({
                "timeframe": "day",
                "start": "2021-12-16T14:00:00+00:00",
                "end": "2021-12-16T16:00:00+02:00"
            });

            let err = from_value::<Time>(full).expect_err("Should error because Start = End");

            assert_eq!(
                "End time should be larger than the Start time",
                err.to_string()
            );
        }
    }
}
