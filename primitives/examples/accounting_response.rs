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
                "0x0000000000000000000000000000000000000000": "10000000000",
                "0x1111111111111111111111111111111111111111": "20000000000",
                "0x2222222222222222222222222222222222222222": "30000000000",
            },
            "spenders": {
                "0x7777777777777777777777777777777777777777": "60000000000",
            },
        });
        assert!(from_value::<AccountingResponse::<CheckedState>>(json).is_ok());
    }
}
