use primitives::{sentry::campaign_create::CreateCampaign, test_util::DUMMY_CAMPAIGN, CampaignId};
use std::str::FromStr;
use serde_json::json;

fn main() {
    // CreateCampaign in an HTTP request
    {
        let create_campaign = CreateCampaign::from_campaign_erased(DUMMY_CAMPAIGN.clone(), None);

        let create_campaign_as_json_str = json!("{
            \"id\":null,
            \"channel\":{
                \"leader\":\"0x80690751969B234697e9059e04ed72195c3507fa\",
                \"follower\":\"0xf3f583AEC5f7C030722Fe992A5688557e1B86ef7\",
                \"guardian\":\"0xe061E1EB461EaBE512759aa18A201B20Fe90631D\",
                \"token\":\"0x2BCaf6968aEC8A3b5126FBfAb5Fd419da6E8AD8E\",
                \"nonce\":\"987654321\"
            },
            \"creator\":\"0xaCBaDA2d5830d1875ae3D2de207A1363B316Df2F\",
            \"budget\":\"100000000000\",
            \"validators\":[
                {
                    \"id\":\"0x80690751969B234697e9059e04ed72195c3507fa\",
                    \"fee\":\"2000000\",
                    \"url\":\"http://localhost:8005\"
                },
                {
                    \"id\":\"0xf3f583AEC5f7C030722Fe992A5688557e1B86ef7\",
                    \"fee\":\"3000000\",
                    \"url\":\"http://localhost:8006\"
                }
            ],
            \"title\":\"Dummy Campaign\",
            \"pricingBounds\":{\"CLICK\":{\"min\":\"0\",\"max\":\"0\"},\"IMPRESSION\":{\"min\":\"1\",\"max\":\"10\"}},
            \"eventSubmission\":{\"allow\":[]},
            \"targetingRules\":[],
            \"created\":1612162800000,
            \"active_to\":4073414400000
        }");

        let create_campaign_from_json = serde_json::from_str(create_campaign_as_json_str).expect("should deserialize");

        assert_eq!(create_campaign, create_campaign_from_json);
    }

    // CreateCampaign with a provided ID
    {
        let mut create_campaign = CreateCampaign::from_campaign_erased(DUMMY_CAMPAIGN.clone(), None);
        create_campaign.id = Some(CampaignId::from_str("0x936da01f9abd4d9d80c702af85c822a8").expect("Should be valid id"));

        let create_campaign_as_json_str = "{
            \"id\":\"0x936da01f9abd4d9d80c702af85c822a8\",
            \"channel\":{
                \"leader\":\"0x80690751969B234697e9059e04ed72195c3507fa\",
                \"follower\":\"0xf3f583AEC5f7C030722Fe992A5688557e1B86ef7\",
                \"guardian\":\"0xe061E1EB461EaBE512759aa18A201B20Fe90631D\",
                \"token\":\"0x2BCaf6968aEC8A3b5126FBfAb5Fd419da6E8AD8E\",
                \"nonce\":\"987654321\"
            },
            \"creator\":\"0xaCBaDA2d5830d1875ae3D2de207A1363B316Df2F\",
            \"budget\":\"100000000000\",
            \"validators\":[
                {
                    \"id\":\"0x80690751969B234697e9059e04ed72195c3507fa\",
                    \"fee\":\"2000000\",
                    \"url\":\"http://localhost:8005\"
                },
                {
                    \"id\":\"0xf3f583AEC5f7C030722Fe992A5688557e1B86ef7\",
                    \"fee\":\"3000000\",
                    \"url\":\"http://localhost:8006\"
                }
            ],
            \"title\":\"Dummy Campaign\",
            \"pricingBounds\":{\"CLICK\":{\"min\":\"0\",\"max\":\"0\"},\"IMPRESSION\":{\"min\":\"1\",\"max\":\"10\"}},
            \"eventSubmission\":{\"allow\":[]},
            \"targetingRules\":[],
            \"created\":1612162800000,
            \"active_to\":4073414400000
        }";

        let create_campaign_from_json = serde_json::from_str(create_campaign_as_json_str).expect("should deserialize");

        assert_eq!(create_campaign, create_campaign_from_json);
    }
}