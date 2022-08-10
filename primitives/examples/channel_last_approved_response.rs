use primitives::{balances::UncheckedState, sentry::LastApprovedResponse};
use serde_json::{from_value, json};

fn main() {
    // An empty response - Channel is brand new and neither an NewState nor a ApproveState
    // have been generated yet.
    // This is threated the same as `with_heartbeat=false` in the route.
    {
        let empty_json = json!({});

        let empty_expected = LastApprovedResponse::<UncheckedState> {
            last_approved: None,
            heartbeats: None,
        };

        assert_eq!(
            empty_expected,
            from_value(empty_json).expect("Should deserialize")
        );
    }

    // Response without `with_heartbeat=true` query parameter
    // or with a `with_heartbeat=false` query parameter
    {
        let response_json = json!({
            "lastApproved": {
                "newState": {
                    "from": "0x80690751969B234697e9059e04ed72195c3507fa",
                    "received": "2022-08-09T14:45:38.090Z",
                    "msg": {
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
                    }
                },
                "approveState": {
                    "from": "0xf3f583AEC5f7C030722Fe992A5688557e1B86ef7",
                    "received": "2022-08-09T14:45:43.110Z",
                    "msg": {
                        "type": "ApproveState",
                        "stateRoot": "a1e2f6ee08185ae06e3212e56ad1e0fcbae95ac8939871eb96e1ee3016234321",
                        "signature": "0xb2ce0010ad5867a4fb4acbde6525c261d76b592d290cb22af120573565168a2e49381e84d4f409c0989fa171dd687bf68b7eeff5b595c845cec8e9b8b1738dbd1c",
                        "isHealthy": true
                    }
                }
            }
        });

        let response: LastApprovedResponse<UncheckedState> =
            from_value(response_json).expect("Should deserialize");
        assert!(response.heartbeats.is_none());
    }

    {
        let response_json = json!({
            "lastApproved": {
                "newState": {
                    "from": "0x80690751969B234697e9059e04ed72195c3507fa",
                    "received": "2022-08-09T14:45:38.090Z",
                    "msg": {
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
                    }
                },
                "approveState": {
                    "from": "0xf3f583AEC5f7C030722Fe992A5688557e1B86ef7",
                    "received": "2022-08-09T14:45:43.110Z",
                    "msg": {
                        "type": "ApproveState",
                        "stateRoot": "a1e2f6ee08185ae06e3212e56ad1e0fcbae95ac8939871eb96e1ee3016234321",
                        "signature": "0xb2ce0010ad5867a4fb4acbde6525c261d76b592d290cb22af120573565168a2e49381e84d4f409c0989fa171dd687bf68b7eeff5b595c845cec8e9b8b1738dbd1c",
                        "isHealthy": true
                    }
                }
            },
            "heartbeats": [
                {
                    "from": "0x80690751969B234697e9059e04ed72195c3507fa",
                    "received": "2022-08-09T14:45:58.110Z",
                    "msg": {
                        "type": "Heartbeat",
                        "signature": "0xc96ed701fc53e27cb28b323c26060273a5bdbad07c4b5470d8090c6a8b03954422b5d0dcc791cc47b5e4186045f60971d7e8e4d69b380cf250fe416cf4aac6901b",
                        "stateRoot": "888f915d6b84ccfa80d3cc6536021efab05f63293ddfb66f1bb1c191909d1372",
                        "timestamp": "2022-08-09T14:45:58.097376454Z"
                    }
                },
                {
                    "from": "0x80690751969B234697e9059e04ed72195c3507fa",
                    "received": "2022-08-09T14:45:28.090Z",
                    "msg": {
                        "type": "Heartbeat",
                        "signature": "0x3f780e2abe0d704428a7921c2f18c070ad503953ef248f533b4ad13fa97c239c5e43a9f3db5077b24d4912cb13367337dc4a6c26976a15811a728e316e7275c41c",
                        "stateRoot": "799c72322f9c35840c4bf41045df2623b33a97a5dfa3994022389ddf8930aac6",
                        "timestamp": "2022-08-09T14:45:28.084928288Z"
                    }
                },
                {
                    "from": "0xf3f583AEC5f7C030722Fe992A5688557e1B86ef7",
                    "received": "2022-08-09T14:46:03.170Z",
                    "msg": {
                        "type": "Heartbeat",
                        "signature": "0xd4ce3c8e1cc8ab690cddc2a6cd229311e91e13a53e59b1ec80d8f877afd241af2c86c5fade37be5057d36c8fc5e69d3222b49b98bf686ee00e73005cc280ebc41b",
                        "stateRoot": "d0dc740d3352cdd7da9b823aa4051830f9757fae66c553a88176ce0001e378fb",
                        "timestamp": "2022-08-09T14:46:03.127887835Z"
                    }
                },
                {
                    "from": "0xf3f583AEC5f7C030722Fe992A5688557e1B86ef7",
                    "received": "2022-08-09T14:45:28.160Z",
                    "msg": {
                        "type": "Heartbeat",
                        "signature": "0x3afda200de4ac36d5c8f1a53da0ffdca5077b556a53fb56bb9a79def1c06f972547b0099731b1ac9b4a26c183e2ea66b8cd1759cdc1513e3436d182e9592ae0e1b",
                        "stateRoot": "fa8f11b8aa6322905846f96219c855920b4449b18f0ceea97552e3880c5e4a9a",
                        "timestamp": "2022-08-09T14:45:28.121225273Z"
                    }
                }
            ]
        });

        let response: LastApprovedResponse<UncheckedState> =
            from_value(response_json).expect("Should deserialize");

        assert!(response.heartbeats.is_some());
    }
}
