use serde::{Deserialize, Serialize};


#[derive(Debug, Deserialize)]
pub struct AnalyticsQuery {
    #[serde(default = "default_limit")]
    pub limit: u32,
    #[serde(default = "default_event_type")]
    pub event_type: String,
    #[serde(default = "default_metric")]
    pub metric: String,
    #[serde(default = "default_timeframe")]
    pub timeframe: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AnalyticsResponse {
    time: u32,
    value: String,
}

#[cfg(feature = "postgres")]
pub mod postgres {
    use tokio_postgres::Row;

    impl From<&Row> for AnalyticsResponse {
        fn from(row: &Row) -> Self {
            Self {
                time: row.get("time"),
                value: row.get("value"),
            }
        }
    }
}

impl AnalyticsQuery {
    pub fn is_valid(&self) -> Result<(), String> {
        let valid_event_types = ["IMPRESSION"];
        let valid_metric = ["eventPayouts", "eventCounts"];
        let valid_timeframe = ["year", "month", "week", "day", "hour"];

        if !valid_event_types.iter().any(|e| *e == &self.event_type[..]) {
            Err(format!(
                "invalid event_type, possible values are: {}",
                valid_event_types.join(" ,")
            ))
        } else if !valid_metric.iter().any(|e| *e == &self.metric[..]) {
            Err(format!(
                "invalid metric, possible values are: {}",
                valid_metric.join(" ,")
            ))
        } else if !valid_timeframe.iter().any(|e| *e == &self.timeframe[..]) {
            Err(format!(
                "invalid timeframe, possible values are: {}",
                valid_timeframe.join(" ,")
            ))
        } else {
            Ok(())
        }
    }
}

fn default_limit() -> u32 {
    100
}

fn default_event_type() -> String {
    "IMPRESSION".into()
}

fn default_metric() -> String {
    "eventCounts".into()
}

fn default_timeframe() -> String {
    "hour".into()
}
