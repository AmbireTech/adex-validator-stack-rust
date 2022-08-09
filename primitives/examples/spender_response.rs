use primitives::{sentry::SpenderResponse};
use serde_json::{from_value, json};

fn main() {
    let json = json!({
        "spender": {
            "totalDeposited": "10000000000",
            "totalSpent": "100000000",
        },
    });
    assert!(from_value::<SpenderResponse>(json).is_ok());
}
