use primitives::{balances::CheckedState, sentry::AccountingResponse};
use serde_json::{from_value, json};

fn main() {
    // Empty balances
    {
        let json = json!({
          "earners": {},
          "spenders": {},
        });
        assert!(from_value::<AccountingResponse::<CheckedState>>(json).is_ok());
    }

    // Non-empty balances
    {
        // earners sum and spenders sum should always match since balances are CheckedState
        let json = json!({
            "earners": {
                "0x80690751969B234697e9059e04ed72195c3507fa": "10000000000",
                "0xf3f583AEC5f7C030722Fe992A5688557e1B86ef7": "20000000000",
                "0xE882ebF439207a70dDcCb39E13CA8506c9F45fD9": "30000000000",
            },
            "spenders": {
                "0xaCBaDA2d5830d1875ae3D2de207A1363B316Df2F": "60000000000",
            },
        });
        assert!(from_value::<AccountingResponse::<CheckedState>>(json).is_ok());
    }
}
