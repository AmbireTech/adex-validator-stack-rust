use primitives::Config;
use serde_json::{from_value, json};

fn main() {
    let json = json!({
        "max_channels":512,
        "channels_find_limit":200,
        "campaigns_find_limit":200,
        "spendable_find_limit":200,
        "wait_time":500,
        "msgs_find_limit":10,
        "analytics_find_limit":5000,
        "analytics_maxtime":20000,
        "heartbeat_time":30000,
        "health_threshold_promilles":950,
        "health_unsignable_promilles":750,
        "propagation_timeout":2000,
        "fetch_timeout":5000,
        "all_campaigns_timeout":5000,
        "channel_tick_timeout":8000,
        "ip_rate_limit":{"type":"ip","timeframe":1200000},
        "creators_whitelist":[],
        "validators_whitelist":[],
        "admins":["0x80690751969B234697e9059e04ed72195c3507fa"],
        "chain":{
            "Ganache #1337": {
                "chain_id":1337,
                "rpc":"http://localhost:1337/",
                "outpace":"0xAbc27d46a458E2e49DaBfEf45ca74dEDBAc3DD06",
                "token":{
                    "Mocked TOKEN 1337":{
                        "min_campaign_budget":"1000000000000000000",
                        "min_validator_fee":"1000000000000",
                        "precision":18,
                        "address":"0x2BCaf6968aEC8A3b5126FBfAb5Fd419da6E8AD8E"
                    }
                }
            },
            "Ganache #1":{
                "chain_id":1,
                "rpc":"http://localhost:8545/",
                "outpace":"0x26CBc2eAAe377f6Ac4b73a982CD1125eF4CEC96f",
                "token":{
                    "Mocked TOKEN 1":{
                        "min_campaign_budget":"1000000000000000000",
                        "min_validator_fee":"1000000000000",
                        "precision":18,
                        "address":"0x12a28f2bfBFfDf5842657235cC058242f40fDEa6"
                    }
                }
            }
        },
        "platform":{
            "url":"https://platform.adex.network/",
            "keep_alive_interval":1200000
        },
        "limits":{
            "units_for_slot":{
                "max_campaigns_earning_from":25,
                "global_min_impression_price":"1000000"
            }
        }
    });
    let val = from_value::<Config>(json).expect("should convert");
    // assert!(from_value::<Config>(json).is_ok());
}
