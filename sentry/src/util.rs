#[cfg(test)]
pub mod tests;

pub mod serde {
    pub mod ts_milliseconds_option {
        use chrono::serde::ts_milliseconds::deserialize as from_ts_milliseconds;
        use chrono::serde::ts_milliseconds::serialize as to_ts_milliseconds;
        use chrono::{DateTime, Utc};
        use serde::{Deserializer, Serializer};

        pub fn serialize<S>(opt: &Option<DateTime<Utc>>, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            match *opt {
                Some(ref dt) => to_ts_milliseconds(dt, serializer),
                None => serializer.serialize_none(),
            }
        }

        pub fn deserialize<'de, D>(de: D) -> Result<Option<DateTime<Utc>>, D::Error>
        where
            D: Deserializer<'de>,
        {
            from_ts_milliseconds(de).map(Some)
        }
    }
}
