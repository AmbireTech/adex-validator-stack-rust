use primitives::sentry::ValidatorMessagesCreateRequest;
use serde_json::{from_value, json};

fn main() {
    // This request is generated using the Ethereum Adapter
    let request_json = json!({
        "messages": [
            {
                "type": "ApproveState",
                "stateRoot": "a1e2f6ee08185ae06e3212e56ad1e0fcbae95ac8939871eb96e1ee3016234321",
                "signature": "0xb2ce0010ad5867a4fb4acbde6525c261d76b592d290cb22af120573565168a2e49381e84d4f409c0989fa171dd687bf68b7eeff5b595c845cec8e9b8b1738dbd1c",
                "isHealthy": true
            },
            {
                "type": "NewState",
                "stateRoot": "a1e2f6ee08185ae06e3212e56ad1e0fcbae95ac8939871eb96e1ee3016234321",
                "signature": "0x508bef21e91d5337ad71791503748fe0d7ee7592db90179be6f948570290d00b72e103b0d262452809ace183ebf83375072b4a359b6e441f2ad6f58b8552c8fa1b",
                "earners": {
                    "0x80690751969B234697e9059e04ed72195c3507fa": "5",
                    "0xE882ebF439207a70dDcCb39E13CA8506c9F45fD9": "100000",
                    "0xf3f583AEC5f7C030722Fe992A5688557e1B86ef7": "3"
                },
                "spenders": {
                    "0xaCBaDA2d5830d1875ae3D2de207A1363B316Df2F": "100008"
                }
            },
            {
                "type": "Heartbeat",
                "signature": "0x3afda200de4ac36d5c8f1a53da0ffdca5077b556a53fb56bb9a79def1c06f972547b0099731b1ac9b4a26c183e2ea66b8cd1759cdc1513e3436d182e9592ae0e1b",
                "stateRoot": "fa8f11b8aa6322905846f96219c855920b4449b18f0ceea97552e3880c5e4a9a",
                "timestamp": "2022-08-09T14:45:28.121225273Z"
            }
        ]
    });

    let request: ValidatorMessagesCreateRequest =
        from_value(request_json).expect("Should deserialize");

    println!("{request:#?}");
}
