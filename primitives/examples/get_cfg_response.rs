use primitives::{config::GANACHE_CONFIG, Config};
use serde_json::{from_value, json};

fn main() {
    let json = json!({
      "creators_whitelist": [
        "0xaCBaDA2d5830d1875ae3D2de207A1363B316Df2F",
        "0xDd589B43793934EF6Ad266067A0d1D4896b0dff0",
        "0xE882ebF439207a70dDcCb39E13CA8506c9F45fD9",
        "0x541b401362Ea1D489D322579552B099e801F3632"
      ],
      "validators_whitelist": [
        "0x80690751969B234697e9059e04ed72195c3507fa",
        "0xf3f583AEC5f7C030722Fe992A5688557e1B86ef7",
        "0x6B83e7D6B72c098d48968441e0d05658dc17Adb9"
      ],
      "chain": {
        "Ganache #1337": {
          "chain_id": 1337,
          "rpc": "http://localhost:1337/",
          "outpace": "0xAbc27d46a458E2e49DaBfEf45ca74dEDBAc3DD06",
          "token": {
            "Mocked TOKEN 1337": {
              "min_campaign_budget": "1000000000000000000",
              "min_validator_fee": "1000000000000",
              "precision": 18,
              "address": "0x2BCaf6968aEC8A3b5126FBfAb5Fd419da6E8AD8E"
            }
          }
        },
        "Ganache #1": {
          "chain_id": 1,
          "rpc": "http://localhost:8545/",
          "outpace": "0x26CBc2eAAe377f6Ac4b73a982CD1125eF4CEC96f",
          "token": {
            "Mocked TOKEN 1": {
              "min_campaign_budget": "1000000000000000000",
              "min_validator_fee": "1000000000000",
              "precision": 18,
              "address": "0x12a28f2bfBFfDf5842657235cC058242f40fDEa6"
            }
          }
        }
      },
      "limits": {
        "units_for_slot": {
          "max_campaigns_earning_from": 25,
          "global_min_impression_price": "10"
        },
        "channels_find": 200,
        "campaigns_find": 200,
        "spendable_find": 200,
        "msgs_find": 10,
        "analytics_find": 5000,
        "ip_rate_limit": {
          "type": "ip",
          "timeframe": 1200000
        }
      },
      "sentry": {
        "analytics_maxtime": 20000,
        "admins": [
          "0x80690751969B234697e9059e04ed72195c3507fa"
        ],
        "platform": {
          "url": "https://platform.adex.network/",
          "keep_alive_interval": 1200000
        },
      },
      "worker": {
        "max_channels": 512,
        "wait_time": 500,
        "heartbeat_time": 30000,
        "health_threshold_promilles": 950,
        "health_unsignable_promilles": 750,
        "timeouts": {
          "propagation": 2000,
          "fetch": 5000,
          "all_campaigns": 5000,
          "channel_tick": 8000,
        }
      }
    });
    assert_eq!(
        from_value::<Config>(json).expect("Should deserialize"),
        GANACHE_CONFIG.clone()
    );
}
