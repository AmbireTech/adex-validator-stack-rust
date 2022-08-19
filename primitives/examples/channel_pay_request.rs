use primitives::sentry::ChannelPayRequest;
use serde_json::json;

fn main() {
    let channel_pay_json = json!({
      "payouts": {
        "0x80690751969B234697e9059e04ed72195c3507fa": "10000000000",
        "0xf3f583AEC5f7C030722Fe992A5688557e1B86ef7": "20000000000",
        "0x0e880972A4b216906F05D67EeaaF55d16B5EE4F1": "30000000000",
      },
    });

    let channel_pay_json = serde_json::to_string(&channel_pay_json).expect("should serialize");

    assert!(serde_json::from_str::<ChannelPayRequest>(&channel_pay_json).is_ok());
}
