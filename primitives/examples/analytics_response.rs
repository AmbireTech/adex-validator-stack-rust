use primitives::sentry::AnalyticsResponse;
use serde_json::{from_value, json};

fn main() {
    let json = json!({
      "analytics": [{
        "time": 1659592800,
        "value": "3",
        "segment": null
      },
      {
        "time": 1659592800,
        "value": "10000000000",
        "segment": null
      },
      {
        "time": 1659592800,
        "value": "100000000",
        "segment": "country"
      }],
      "totalPages": 1,
      "page": 0
    });

    assert!(from_value::<AnalyticsResponse>(json).is_ok());
}
