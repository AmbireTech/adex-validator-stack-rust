use primitives::AdSlot;
use serde_json::{from_value, json};

fn main() {
    let json = json!({
        "ipfs": "QmcUVX7fvoLMM93uN2bD3wGTH8MXSxeL8hojYfL2Lhp7mR",
        "type": "legacy_250x250",
        "minPerImpression": {
            // `Mocked TOKEN 1: 0.1`
            "0x12a28f2bfBFfDf5842657235cC058242f40fDEa6": "10000000"
        },
        "rules": [],
        "fallbackUnit": null,
        "owner": "0xE882ebF439207a70dDcCb39E13CA8506c9F45fD9",
        // milliseconds
        "created": "1564372800000",
        "title": "Test AdSlot",
        "website": "adex.network",
        "archived": false,
    });

    // let ad_slot = AdSlot {
    //     ipfs: DUMMY_IPFS[0],
    //     ad_type: "legacy_250x250".to_string(),
    //     archived: false,
    //     created: Utc.ymd(2019, 7, 29).and_hms(7, 0, 0),
    //     description: Some("Test slot for running integration tests".to_string()),
    //     fallback_unit: Some(fallback_unit.ipfs),
    //     min_per_impression: Some(
    //         [
    //             (
    //                 GANACHE_INFO_1.tokens["Mocked TOKEN 1"].address,
    //                 UnifiedNum::from_whole(0.010),
    //             ),
    //             (
    //                 GANACHE_INFO_1337.tokens["Mocked TOKEN 1337"].address,
    //                 UnifiedNum::from_whole(0.001),
    //             ),
    //         ]
    //         .into_iter()
    //         .collect(),
    //     ),
    //     modified: Some(Utc.ymd(2019, 7, 29).and_hms(7, 0, 0)),
    //     owner: IDS[&PUBLISHER],
    //     title: Some("Test slot 1".to_string()),
    //     website: Some("https://adex.network".to_string()),
    //     rules: Rules::default(),
    // }

    // let expected = AdSlot {
    //     ipfs: DUMMY_IPFS[0],
    //     ad_type: String,
    //     min_per_impression: Option<HashMap<Address, UnifiedNum>>,
    //     rules: Rules::default(),
    //      fallback_unit: Some(),
    //      owner: ValidatorId,
    //      created: DateTime<Utc>,
    //      title: Option<String>,
    //      description: Option<String>,
    //      website: Option<String>,
    //     archived: false,
    //     modified: Option<DateTime<Utc>>,
    // };

    assert!(from_value::<AdSlot>(json).is_ok());
}
