use primitives::sentry::ValidatorMessagesCreateRequest;
use serde_json::json;
use std::str::FromStr;

fn main() {
    let messages_json = json!({
        "messages": [
            {
                "type": "ApproveState",
                "stateRoot": "b1a4fc6c1a1e1ab908a487e504006edcebea297f61b4b8ce6cad3b29e29454cc",
                "signature": "0xf3f583AEC5f7C030722Fe992A5688557e1B86ef7",
                "isHealthy": true,
            },
            { // RejectState
                "type": "RejectState",
                "reason": "rejected",
                "stateRoot": "b1a4fc6c1a1e1ab908a487e504006edcebea297f61b4b8ce6cad3b29e29454cc",
                "signature": "0xf3f583AEC5f7C030722Fe992A5688557e1B86ef7",
                "earners": {},
                "spenders": {},
                "timestamp": "2022-08-09T16:07:18.136334Z",
            },
            { // NewState
                "type": "NewState",
                "stateRoot": "b1a4fc6c1a1e1ab908a487e504006edcebea297f61b4b8ce6cad3b29e29454cc",
                "signature": "0x80690751969B234697e9059e04ed72195c3507fa",
                "earners": {
                    "0x0e880972A4b216906F05D67EeaaF55d16B5EE4F1": "2000",
                    "0xE882ebF439207a70dDcCb39E13CA8506c9F45fD9": "2000",
                },
                "spenders": {
                    "0xaCBaDA2d5830d1875ae3D2de207A1363B316Df2F": "2000",
                    "0xDd589B43793934EF6Ad266067A0d1D4896b0dff0": "2000",
                },
            },
            { // Heartbeat
                "type": "Heartbeat",
                "stateRoot": "b1a4fc6c1a1e1ab908a487e504006edcebea297f61b4b8ce6cad3b29e29454cc",
                "signature": "0xf3f583AEC5f7C030722Fe992A5688557e1B86ef7",
                "timestamp": "1612162800000",
            }
        ]
    });

    let messages_json = serde_json::to_string(&messages_json).expect("should serialize");

    // assert!(serde_json::from_str::<ValidatorMessagesCreateRequest>(&messages_json).is_ok());
}
