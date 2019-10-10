pub mod tests {
    use rand::seq::SliceRandom;
    use rand::thread_rng;

    pub mod prep_db;
    pub mod time;

    #[inline]
    pub fn take_one<'a, T: ?Sized>(list: &[&'a T]) -> &'a T {
        let mut rng = thread_rng();
        list.choose(&mut rng).expect("take_one got empty list")
    }
}

pub mod serde {
    pub mod ts_milliseconds_option {
        use chrono::serde::ts_milliseconds::deserialize as from_ts_milliseconds;
        use chrono::serde::ts_milliseconds::serialize as to_ts_milliseconds;
        use chrono::{DateTime, Utc};
        use serde::{de, Serializer};
        use std::fmt;

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
            D: de::Deserializer<'de>,
        {
            Ok(de
                .deserialize_option(OptionMilliSecondsTimestampVisitor)
                .map(|opt| opt.map(|dt| dt.with_timezone(&Utc))))?
        }

        struct OptionMilliSecondsTimestampVisitor;

        impl<'de> de::Visitor<'de> for OptionMilliSecondsTimestampVisitor {
            type Value = Option<DateTime<Utc>>;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a unix timestamp in milliseconds or none")
            }

            /// Deserialize a timestamp in seconds since the epoch
            fn visit_none<E>(self) -> Result<Option<DateTime<Utc>>, E>
            where
                E: de::Error,
            {
                Ok(None)
            }

            /// Deserialize a timestamp in seconds since the epoch
            fn visit_some<D>(self, de: D) -> Result<Option<DateTime<Utc>>, D::Error>
            where
                D: de::Deserializer<'de>,
            {
                from_ts_milliseconds(de).map(Some)
            }
        }
    }
}
