use primitives::{sentry::campaign_modify::ModifyCampaign, unified_num::FromWhole, UnifiedNum};
use serde_json::json;
use std::str::FromStr;

fn main() {
    {
        let modify_campaign = ModifyCampaign {
            ad_units: None,
            budget: Some(UnifiedNum::from_whole(100)),
            validators: None,
            title: None,
            pricing_bounds: None,
            event_submission: None,
            targeting_rules: None,
        };

        let modify_campaign_json = json!({
            "ad_units": null,
            "budget": "10000000000",
            "validators": null,
            "title": null,
            "pricing_bounds": null,
            "event_submission": null,
            "targeting_rules": null,
        });

        let modify_campaign_json =
            serde_json::to_string(&modify_campaign_json).expect("should serialize");
        let deserialized: ModifyCampaign =
            serde_json::from_str(&modify_campaign_json).expect("should deserialize");

        assert_eq!(modify_campaign, deserialized);
    }
}
