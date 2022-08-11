use primitives::sentry::AllSpendersResponse;
use serde_json::{from_value, json};

fn main() {
    let json = json!({
        "spenders": {
            "0xaCBaDA2d5830d1875ae3D2de207A1363B316Df2F": {
                "totalDeposited": "10000000000",
                "totalSpent": "100000000",
            },
            "0xDd589B43793934EF6Ad266067A0d1D4896b0dff0": {
                "totalDeposited": "90000000000",
                "totalSpent": "20000000000",
            },
            "0x541b401362Ea1D489D322579552B099e801F3632": {
                "totalDeposited": "1000000000",
                "totalSpent": "1000000000",
            },
        },
        "totalPages": 1,
        "page": 0
    });
    assert!(from_value::<AllSpendersResponse>(json).is_ok());
}
