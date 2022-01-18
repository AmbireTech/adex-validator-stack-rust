use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
pub struct EventSubmission {
    #[serde(default)]
    pub allow: Vec<Rule>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Rule {
    #[serde(default)]
    pub uids: Option<Vec<String>>,
    #[serde(default)]
    pub rate_limit: Option<RateLimit>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RateLimit {
    /// "ip", "uid"
    #[serde(rename = "type")]
    pub limit_type: String,
    /// in milliseconds
    #[serde(rename = "timeframe", with = "serde_millis")]
    pub time_frame: Duration,
}

#[cfg(feature = "postgres")]
mod postgres {
    use super::EventSubmission;

    use bytes::BytesMut;
    use std::error::Error;
    use tokio_postgres::types::{accepts, to_sql_checked, FromSql, IsNull, Json, ToSql, Type};

    impl<'a> FromSql<'a> for EventSubmission {
        fn from_sql(ty: &Type, raw: &'a [u8]) -> Result<Self, Box<dyn Error + Sync + Send>> {
            let json = <Json<Self> as FromSql>::from_sql(ty, raw)?;

            Ok(json.0)
        }

        accepts!(JSONB);
    }

    impl ToSql for EventSubmission {
        fn to_sql(
            &self,
            ty: &Type,
            w: &mut BytesMut,
        ) -> Result<IsNull, Box<dyn Error + Sync + Send>> {
            Json(self).to_sql(ty, w)
        }

        accepts!(JSONB);
        to_sql_checked!();
    }
}
