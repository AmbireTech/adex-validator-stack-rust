use primitives::{unified_num::FromWhole, UnifiedNum};
use sentry::routes::channel::ChannelDummyDeposit;
use serde_json::{from_value, json};

fn main() {
    let request_json = json!({
        "channel": {
            "leader": "0x80690751969B234697e9059e04ed72195c3507fa",
            "follower": "0xf3f583AEC5f7C030722Fe992A5688557e1B86ef7",
            "guardian": "0xe061E1EB461EaBE512759aa18A201B20Fe90631D",
            "token": "0x2bcaf6968aec8a3b5126fbfab5fd419da6e8ad8e",
            "nonce": "0"
          },
        "deposit": {
            "total": "20000000000000"
        }
    });

    let request: ChannelDummyDeposit = from_value(request_json).expect("Should deserialize");

    assert_eq!(UnifiedNum::from_whole(200000.0), request.deposit.total);

    println!("{request:#?}");
}
