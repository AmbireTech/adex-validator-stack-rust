use primitives::sentry::AllSpendersResponse;
use serde_json::{from_value, json};

fn main() {
    let json = json!({
        "spenders": {
            "0x0000000000000000000000000000000000000000": {
                "totalDeposited": "10000000000",
                "totalSpent": "100000000",
            },
            "0x1111111111111111111111111111111111111111": {
                "totalDeposited": "90000000000",
                "totalSpent": "20000000000",
            },
            "0x2222222222222222222222222222222222222222": {
                "totalDeposited": "1000000000",
                "totalSpent": "1000000000",
            },
        },
        "totalPages": 1,
        "page": 0
    });
    assert!(from_value::<AllSpendersResponse>(json).is_ok());
}
