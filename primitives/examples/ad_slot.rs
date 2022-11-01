use chrono::{TimeZone, Utc};
use primitives::{
    targeting::Rules,
    test_util::{DUMMY_AD_UNITS, DUMMY_IPFS, IDS, PUBLISHER},
    AdSlot,
};
use serde_json::{from_value, json};

fn main() {
    let json = json!({
        "ipfs": "QmcUVX7fvoLMM93uN2bD3wGTH8MXSxeL8hojYfL2Lhp7mR",
        "type": "legacy_300x100",
        "minPerImpression": null,
        "rules": [],
        "fallbackUnit": "Qmasg8FrbuSQpjFu3kRnZF9beg8rEBFrqgi1uXDRwCbX5f",
        "owner": "0xE882ebF439207a70dDcCb39E13CA8506c9F45fD9",
        // milliseconds
        "created": 1564372800000_u64,
        "title": "Test AdSlot",
        "description": null,
        "website": "https://adex.network",
        "archived": false,
        // milliseconds
        "modified": 1564372800000_u64
    });

    let fallback_unit = DUMMY_AD_UNITS[0].ipfs;

    let expected_ad_slot = AdSlot {
        ipfs: DUMMY_IPFS[0],
        ad_type: "legacy_300x100".to_string(),
        min_per_impression: None,
        rules: Rules::default(),
        fallback_unit: Some(fallback_unit),
        owner: IDS[&PUBLISHER],
        created: Utc.ymd(2019, 7, 29).and_hms(4, 0, 0),
        title: Some("Test AdSlot".to_string()),
        description: None,
        website: Some("https://adex.network".to_string()),
        archived: false,
        modified: Some(Utc.ymd(2019, 7, 29).and_hms(4, 0, 0)),
    };

    pretty_assertions::assert_eq!(
        from_value::<AdSlot>(json).expect("Should deserialize"),
        expected_ad_slot
    );
}
