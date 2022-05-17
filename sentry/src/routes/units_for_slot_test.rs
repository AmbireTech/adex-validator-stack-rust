use crate::{platform::PlatformApi, test_util::setup_dummy_app};

use super::*;
use adapter::Dummy;
use chrono::{DateTime, TimeZone, Utc};
use hyper::Body;
use hyper::{
    body::Bytes,
    http::{header::USER_AGENT, request::Request},
};
use primitives::{
    campaign::Pricing,
    platform::AdUnitsResponse,
    supermarket::units_for_slot::response::{Campaign as ResponseCampaign, UnitsWithPrice},
    targeting::Rules,
    targeting::{input, Function, Rule, Value},
    test_util::{DUMMY_AD_UNITS, DUMMY_CAMPAIGN, IDS, LEADER_2, PUBLISHER, PUBLISHER_2},
    AdSlot, BigNum, Channel, ChannelId,
};
use reqwest::Url;
use std::{collections::HashMap, iter::Iterator, str::FromStr};
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

// User Agent OS: Linux (only in `woothee`)
// User Agent Browser Family: Firefox
const TEST_USER_AGENT: &str =
    "Mozilla/5.0 (X11; Ubuntu; Linux x86_64; rv:83.0) Gecko/20100101 Firefox/83.0";
// uses two-letter country codes: https://en.wikipedia.org/wiki/ISO_3166-1_alpha-2
const TEST_CLOUDFLARE_IPCOUNTY: &str = "BG";

/// Uses the Channel AdUnits as UnitsWithPrice for the response
fn get_mock_campaign(campaign: Campaign) -> ResponseCampaign {
    let units_with_price = get_units_with_price(&campaign);
    ResponseCampaign {
        campaign: ResponseCampaign::from(campaign),
        units_with_price,
    }
}

fn get_units_with_price(campaign: &Campaign) -> Vec<UnitsWithPrice> {
    campaign
        .ad_units
        .iter()
        .map(|u| UnitsWithPrice {
            unit: u.into(),
            price: campaign
                .pricing_bounds
                .get(&IMPRESSION)
                .expect("Campaign should have Pricing Bounds for impression")
                .min,
        })
        .collect()
}

fn get_mock_rules(categories: &[&str]) -> Vec<Rule> {
    let get_rule = Function::new_get("adSlot.categories");
    let categories_array = Value::Array(categories.iter().map(|s| Value::new_string(s)).collect());
    let intersects_rule = Function::new_intersects(get_rule, categories_array);
    vec![Function::new_only_show_if(intersects_rule).into()]
}

fn get_test_ad_slot(rules: &[Rule], categories: &[&str]) -> AdSlot {
    AdSlot {
        // TODO: Replace with IPFS for testing
        ipfs: "QmVwXu9oEgYSsL6G1WZtUQy6dEReqs3Nz9iaW4Cq5QLV8C"
            .parse()
            .expect("Valid IPFS"),
        ad_type: "legacy_250x250".to_string(),
        archived: false,
        created: Utc.timestamp(1_564_383_600, 0),
        description: Some("Test slot for running integration tests".to_string()),
        fallback_unit: None,
        min_per_impression: Some(
            vec![(
                "0x89d24A6b4CcB1B6fAA2625fE562bDD9a23260359"
                    .parse()
                    .expect("Valid Address"),
                // 0.0007
                70000.into(),
            )]
            .into_iter()
            .collect(),
        ),
        modified: Some(Utc.timestamp(1_564_383_600, 0)),
        owner: IDS[PUBLISHER],
        title: Some("Test slot 1".to_string()),
        website: Some("https://adex.network".to_string()),
        rules: rules.to_vec(),
    }

    //     AdSlotResponse {
    //         slot: ad_slot,
    //         accepted_referrers: vec![
    //             Url::from_str("https://adex.network").expect("should parse"),
    //             Url::from_str("https://www.adex.network").expect("should parse"),
    //         ],
    //         categories: categories.iter().map(|s| String::from(*s)).collect(),
    //         alexa_rank: Some(1337.0),
    //     }
}

/// `seconds_since_epoch` should be set from the actual response,
/// this ensures that the timestamp will always match in the tests,
/// otherwise random tests will fail with +- 1-2-3 seconds difference
fn get_expected_response(
    campaigns: Vec<Campaign>,
    seconds_since_epoch: DateTime<Utc>,
) -> UnitsForSlotResponse {
    let targeting_input_base = Input {
        ad_view: None,
        global: input::Global {
            ad_slot_id: "QmVwXu9oEgYSsL6G1WZtUQy6dEReqs3Nz9iaW4Cq5QLV8C"
                .parse()
                .expect("Valid IPFS"),
            ad_slot_type: "legacy_250x250".to_string(),
            publisher_id: *PUBLISHER,
            country: Some(TEST_CLOUDFLARE_IPCOUNTY.to_string()),
            event_type: IMPRESSION,
            seconds_since_epoch,
            user_agent_os: Some("Linux".to_string()),
            user_agent_browser_family: Some("Firefox".to_string()),
        },
        ad_unit_id: None,
        balances: None,
        campaign: None,
        ad_slot: Some(input::AdSlot {
            categories: vec!["IAB3".into(), "IAB13-7".into(), "IAB5".into()],
            hostname: "adex.network".to_string(),
            alexa_rank: Some(1337.0),
        }),
    };

    UnitsForSlotResponse {
        targeting_input_base: targeting_input_base.into(),
        accepted_referrers: vec![],
        campaigns,
        fallback_unit: None,
    }
}

fn mock_campaign(rules: &[Rule]) -> Campaign {
    let mut campaign = DUMMY_CAMPAIGN.clone();

    campaign.ad_units = DUMMY_AD_UNITS.to_vec();
    // NOTE: always set the spec.targeting_rules first
    campaign.targeting_rules = Rules(rules.to_vec());
    // override pricing for `IMPRESSION`
    campaign.pricing_bounds.insert(
        IMPRESSION,
        Pricing {
            // 0.0001
            min: 10_000.into(),
            // 0.001
            max: 100_000.into(),
        },
    );
    // Timestamp: 1_606_136_400_000
    campaign.active.from = Some(Utc.ymd(2020, 11, 23).and_hms(15, 0, 0));

    campaign
}

// fn mock_cache_campaign(channel: Channel, status: Status) -> HashMap<ChannelId, Campaign> {
//     let mut campaigns = HashMap::new();

//     let mut campaign = Campaign {
//         channel,
//         status,
//         balances: Default::default(),
//     };
//     campaign
//         .balances
//         .insert(*PUBLISHER, 100_000_000_000_000.into());

//     campaigns.insert(campaign.channel.id, campaign);
//     campaigns
// }

/// Assumes all `Campaign`s are `Active`
/// adds to Balances the `Publisher` address with `1 * 10^14` balance
// fn mock_multiple_cache_campaigns(channels: Vec<Channel>) -> HashMap<ChannelId, Campaign> {
//     let mut campaigns = HashMap::new();

//     for channel in channels {
//         let mut campaign = Campaign {
//             channel,
//             status: Status::Active,
//             balances: Default::default(),
//         };
//         campaign
//             .balances
//             .insert(*PUBLISHER, 100_000_000_000_000.into());

//         campaigns.insert(campaign.channel.id, campaign);
//     }

//     campaigns
// }

/// Sets platform at the `{server_uri}/platform` path of the [`MockServer`]
pub async fn init_app_with_mocked_platform(server: &MockServer) -> Application<Dummy> {
    let platform_url = (server.uri() + "/platform").parse().unwrap();
    let mut dummy_app = setup_dummy_app().await;

    let platform_api =
        PlatformApi::new(platform_url, dummy_app.config.platform.keep_alive_interval)
            .expect("should build test PlatformApi");
    // override the Dummy app PlatformApi
    dummy_app.platform_api = platform_api;

    dummy_app
}

#[tokio::test]
async fn targeting_input() {
    let server = MockServer::start().await;

    let app = init_app_with_mocked_platform(&server).await;

    let categories: [&str; 3] = ["IAB3", "IAB13-7", "IAB5"];
    let rules = get_mock_rules(&categories);
    let campaign = mock_campaign(&rules);

    let platform_ad_units = AdUnitsResponse(campaign.ad_units.clone());
    let mock_slot = get_test_ad_slot(&rules, &categories);

    Mock::given(method("GET"))
        .and(path("/platform/units"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&platform_ad_units))
        .mount(&server)
        .await;

    let request = Request::post("/units-for-slot")
        .header(USER_AGENT, TEST_USER_AGENT)
        .header(CLOUDFLARE_IPCOUNTY_HEADER.clone(), TEST_CLOUDFLARE_IPCOUNTY)
        .body(
            serde_json::to_vec(&RequestBody {
                ad_slot: mock_slot,
                deposit_assets: Some(vec![campaign.channel.token].into_iter().collect()),
            })
            .expect("Should serialize"),
        )
        .unwrap();

    let actual_response = post_units_for_slot(request, &app)
        .await
        .expect("call shouldn't fail with provided data");

    assert_eq!(StatusCode::OK, actual_response.status());

    let units_for_slot: UnitsForSlotResponse =
        serde_json::from_slice(&hyper::body::to_bytes(actual_response).await.unwrap())
            .expect("Should deserialize");

    // we must use the same timestamp as the response, otherwise our tests will fail randomly
    let expected_response = get_expected_response(
        vec![campaign],
        units_for_slot
            .targeting_input_base
            .global
            .seconds_since_epoch
            .clone(),
    );

    pretty_assertions::assert_eq!(
        expected_response.targeting_input_base,
        units_for_slot.targeting_input_base
    );

    assert_eq!(
        expected_response.campaigns.len(),
        units_for_slot.campaigns.len()
    );
    assert_eq!(expected_response.campaigns, units_for_slot.campaigns);
    assert_eq!(
        expected_response.fallback_unit,
        units_for_slot.fallback_unit
    );
}

#[tokio::test]
async fn non_active_campaign() {
    let server = MockServer::start().await;

    let app = init_app_with_mocked_platform(&server).await;

    let categories: [&str; 3] = ["IAB3", "IAB13-7", "IAB5"];
    let rules = get_mock_rules(&categories);
    let campaign = mock_campaign(&rules);

    let platform_ad_units = AdUnitsResponse(campaign.ad_units);
    let mock_slot = get_test_ad_slot(&rules, &categories);

    Mock::given(method("GET"))
        .and(path("/platform/units"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&platform_ad_units))
        .mount(&server)
        .await;

    let request = Request::post("/units-for-slot")
        .header(USER_AGENT, TEST_USER_AGENT)
        .header(CLOUDFLARE_IPCOUNTY_HEADER.clone(), TEST_CLOUDFLARE_IPCOUNTY)
        .body(
            serde_json::to_vec(&RequestBody {
                ad_slot: mock_slot,
                deposit_assets: Some(vec![campaign.channel.token].into_iter().collect()),
            })
            .expect("Should serialize"),
        )
        .unwrap();

    let actual_response = post_units_for_slot(request, &app)
        .await
        .expect("call shouldn't fail with provided data");

    assert_eq!(StatusCode::OK, actual_response.status());

    let units_for_slot: UnitsForSlotResponse =
        serde_json::from_slice(&hyper::body::to_bytes(actual_response).await.unwrap())
            .expect("Should deserialize");

    // we must use the same timestamp as the response, otherwise our tests will fail randomly
    let expected_response = get_expected_response(
        vec![],
        units_for_slot
            .targeting_input_base
            .global
            .seconds_since_epoch
            .clone(),
    );

    pretty_assertions::assert_eq!(
        expected_response.targeting_input_base,
        units_for_slot.targeting_input_base
    );

    assert_eq!(
        expected_response.campaigns.len(),
        units_for_slot.campaigns.len()
    );
    assert_eq!(expected_response.campaigns, units_for_slot.campaigns);
    assert_eq!(
        expected_response.fallback_unit,
        units_for_slot.fallback_unit
    );
}

#[tokio::test]
async fn creator_is_publisher() {
    let server = MockServer::start().await;

    let app = init_app_with_mocked_platform(&server).await;

    let categories: [&str; 3] = ["IAB3", "IAB13-7", "IAB5"];
    let rules = get_mock_rules(&categories);
    let mut campaign = mock_campaign(&rules);
    campaign.creator = *PUBLISHER;

    let platform_ad_units = AdUnitsResponse(campaign.ad_units.clone());
    let mock_slot = get_test_ad_slot(&rules, &categories);

    Mock::given(method("GET"))
        .and(path("/platform/units"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&platform_ad_units))
        .mount(&server)
        .await;

    let request = Request::post("/units-for-slot")
        .header(USER_AGENT, TEST_USER_AGENT)
        .header(CLOUDFLARE_IPCOUNTY_HEADER.clone(), TEST_CLOUDFLARE_IPCOUNTY)
        .body(
            Bytes::from_static(
                &serde_json::to_vec(&RequestBody {
                    ad_slot: mock_slot,
                    deposit_assets: Some(vec![campaign.channel.token].into_iter().collect()),
                })
                .expect("Should serialize"),
            )
            .into(),
        )
        .unwrap();

    let actual_response = post_units_for_slot(request, &app)
        .await
        .expect("call shouldn't fail with provided data");

    assert_eq!(StatusCode::OK, actual_response.status());

    let units_for_slot: UnitsForSlotResponse =
        serde_json::from_slice(&hyper::body::to_bytes(actual_response).await.unwrap())
            .expect("Should deserialize");

    // we must use the same timestamp as the response, otherwise our tests will fail randomly
    let expected_response = get_expected_response(
        vec![],
        units_for_slot
            .targeting_input_base
            .global
            .seconds_since_epoch
            .clone(),
    );

    pretty_assertions::assert_eq!(
        expected_response.targeting_input_base,
        units_for_slot.targeting_input_base
    );

    assert_eq!(
        expected_response.campaigns.len(),
        units_for_slot.campaigns.len()
    );
    assert_eq!(expected_response.campaigns, units_for_slot.campaigns);
    assert_eq!(
        expected_response.fallback_unit,
        units_for_slot.fallback_unit
    );
}

#[tokio::test]
async fn no_ad_units() {
    let server = MockServer::start().await;

    let app = init_app_with_mocked_platform(&server).await;

    let categories: [&str; 3] = ["IAB3", "IAB13-7", "IAB5"];
    let rules = get_mock_rules(&categories);
    let mut campaign = mock_campaign(&rules);
    campaign.ad_units = vec![];

    let platform_ad_units = AdUnitsResponse(campaign.ad_units.clone());
    let mock_slot = get_test_ad_slot(&rules, &categories);

    Mock::given(method("GET"))
        .and(path("/platform/units"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&platform_ad_units))
        .mount(&server)
        .await;

    let request = Request::post("/units-for-slot")
        .header(USER_AGENT, TEST_USER_AGENT)
        .header(CLOUDFLARE_IPCOUNTY_HEADER.clone(), TEST_CLOUDFLARE_IPCOUNTY)
        .body(
            serde_json::to_vec(&RequestBody {
                ad_slot: mock_slot,
                deposit_assets: Some(vec![campaign.channel.token].into_iter().collect()),
            })
            .expect("Should serialize"),
        )
        .unwrap();

    let actual_response = post_units_for_slot(request, &app)
        .await
        .expect("call shouldn't fail with provided data");

    assert_eq!(StatusCode::OK, actual_response.status());

    let units_for_slot: UnitsForSlotResponse =
        serde_json::from_slice(&hyper::body::to_bytes(actual_response).await.unwrap())
            .expect("Should deserialize");

    // we must use the same timestamp as the response, otherwise our tests will fail randomly
    let expected_response = get_expected_response(
        vec![],
        units_for_slot
            .targeting_input_base
            .global
            .seconds_since_epoch
            .clone(),
    );

    pretty_assertions::assert_eq!(
        expected_response.targeting_input_base,
        units_for_slot.targeting_input_base
    );

    assert_eq!(
        expected_response.campaigns.len(),
        units_for_slot.campaigns.len()
    );
    assert_eq!(expected_response.campaigns, units_for_slot.campaigns);
    assert_eq!(
        expected_response.fallback_unit,
        units_for_slot.fallback_unit
    );
}

#[tokio::test]
async fn price_less_than_min_per_impression() {
    let server = MockServer::start().await;

    let app = init_app_with_mocked_platform(&server).await;

    let categories: [&str; 3] = ["IAB3", "IAB13-7", "IAB5"];
    let rules = get_mock_rules(&categories);
    let mut campaign = mock_campaign(&rules);
    campaign
        .pricing_bounds
        .get_mut(&IMPRESSION)
        .expect("Campaign should have IMPRESSION pricing bound")
        // 0.00001
        // should be less than `config.limits.units_for_slot.global_min_impression_price`
        .min = 1_000.into();

    let platform_ad_units = AdUnitsResponse(campaign.ad_units.clone());
    let mock_slot = get_test_ad_slot(&rules, &categories);

    Mock::given(method("GET"))
        .and(path("/platform/units"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&platform_ad_units))
        .mount(&server)
        .await;

    let request = Request::post("/units-for-slot")
        .header(USER_AGENT, TEST_USER_AGENT)
        .header(CLOUDFLARE_IPCOUNTY_HEADER.clone(), TEST_CLOUDFLARE_IPCOUNTY)
        .body(
            serde_json::to_vec(&RequestBody {
                ad_slot: mock_slot,
                deposit_assets: Some(vec![campaign.channel.token].into_iter().collect()),
            })
            .expect("Should serialize"),
        )
        .unwrap();

    let actual_response = post_units_for_slot(request, &app)
        .await
        .expect("call shouldn't fail with provided data");

    assert_eq!(StatusCode::OK, actual_response.status());

    let units_for_slot: UnitsForSlotResponse =
        serde_json::from_slice(&hyper::body::to_bytes(actual_response).await.unwrap())
            .expect("Should deserialize");

    // we must use the same timestamp as the response, otherwise our tests will fail randomly
    let expected_response = get_expected_response(
        vec![],
        units_for_slot
            .targeting_input_base
            .global
            .seconds_since_epoch
            .clone(),
    );

    pretty_assertions::assert_eq!(
        expected_response.targeting_input_base,
        units_for_slot.targeting_input_base
    );

    assert_eq!(
        expected_response.campaigns.len(),
        units_for_slot.campaigns.len()
    );
    assert_eq!(expected_response.campaigns, units_for_slot.campaigns);
    assert_eq!(
        expected_response.fallback_unit,
        units_for_slot.fallback_unit
    );
}

#[tokio::test]
async fn non_matching_deposit_asset() {
    let server = MockServer::start().await;

    let app = init_app_with_mocked_platform(&server).await;

    let categories: [&str; 3] = ["IAB3", "IAB13-7", "IAB5"];
    let rules = get_mock_rules(&categories);
    let mut campaign = mock_campaign(&rules);
    campaign.channel.token = "0x000000000000000000000000000000000000000".parse().unwrap();

    let platform_ad_units = AdUnitsResponse(campaign.ad_units.clone());
    let mock_slot = get_test_ad_slot(&rules, &categories);

    Mock::given(method("GET"))
        .and(path("/platform/units"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&platform_ad_units))
        .mount(&server)
        .await;

    let request = Request::post("/units-for-slot/")
        .header(USER_AGENT, TEST_USER_AGENT)
        .header(CLOUDFLARE_IPCOUNTY_HEADER.clone(), TEST_CLOUDFLARE_IPCOUNTY)
        .body(
            serde_json::to_vec(&RequestBody {
                ad_slot: mock_slot,
                deposit_assets: Some(vec![DUMMY_CAMPAIGN.channel.token].into_iter().collect()),
            })
            .expect("Should serialize"),
        )
        .unwrap();

    let actual_response = post_units_for_slot(request, &app)
        .await
        .expect("call shouldn't fail with provided data");

    assert_eq!(StatusCode::OK, actual_response.status());

    let units_for_slot: UnitsForSlotResponse =
        serde_json::from_slice(&hyper::body::to_bytes(actual_response).await.unwrap())
            .expect("Should deserialize");

    // we must use the same timestamp as the response, otherwise our tests will fail randomly
    let expected_response = get_expected_response(
        vec![],
        units_for_slot
            .targeting_input_base
            .global
            .seconds_since_epoch
            .clone(),
    );

    pretty_assertions::assert_eq!(
        expected_response.targeting_input_base,
        units_for_slot.targeting_input_base
    );

    assert_eq!(
        expected_response.campaigns.len(),
        units_for_slot.campaigns.len()
    );
    assert_eq!(expected_response.campaigns, units_for_slot.campaigns);
    assert_eq!(
        expected_response.fallback_unit,
        units_for_slot.fallback_unit
    );
}

#[tokio::test]
async fn multiple_campaigns() {
    let server = MockServer::start().await;

    let app = init_app_with_mocked_platform(&server).await;

    let categories: [&str; 3] = ["IAB3", "IAB13-7", "IAB5"];
    let rules = get_mock_rules(&categories);
    let campaign = mock_campaign(&rules);

    let non_matching_categories: [&str; 3] = ["IAB2", "IAB9-WS1", "IAB19"];
    let non_matching_rules = get_mock_rules(&non_matching_categories);
    let mut non_matching_campaign = mock_campaign(&non_matching_rules);
    non_matching_campaign.channel.leader = LEADER_2;
    non_matching_campaign.creator = *PUBLISHER;

    let platform_ad_units = AdUnitsResponse(campaign.ad_units.clone());
    let mock_slot = get_test_ad_slot(&rules, &categories);

    Mock::given(method("GET"))
        .and(path("/platform/units"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&platform_ad_units))
        .mount(&server)
        .await;

    let campaign = get_mock_campaign(campaign.clone());

    let request = Request::post("/units-for-slot")
        .header(USER_AGENT, TEST_USER_AGENT)
        .header(CLOUDFLARE_IPCOUNTY_HEADER.clone(), TEST_CLOUDFLARE_IPCOUNTY)
        .body(
            serde_json::to_vec(&RequestBody {
                ad_slot: mock_slot,
                deposit_assets: Some(vec![DUMMY_CAMPAIGN.channel.token].into_iter().collect()),
            })
            .expect("Should serialize"),
        )
        .unwrap();

    let actual_response = post_units_for_slot(request, &app)
        .await
        .expect("call shouldn't fail with provided data");

    assert_eq!(StatusCode::OK, actual_response.status());

    let units_for_slot: UnitsForSlotResponse =
        serde_json::from_slice(&hyper::body::to_bytes(actual_response).await.unwrap())
            .expect("Should deserialize");

    // we must use the same timestamp as the response, otherwise our tests will fail randomly
    let expected_response = get_expected_response(
        vec![campaign],
        units_for_slot
            .targeting_input_base
            .global
            .seconds_since_epoch
            .clone(),
    );

    pretty_assertions::assert_eq!(
        expected_response.targeting_input_base,
        units_for_slot.targeting_input_base
    );

    assert_eq!(
        expected_response.campaigns.len(),
        units_for_slot.campaigns.len()
    );
    assert_eq!(expected_response.campaigns, units_for_slot.campaigns);
    assert_eq!(
        expected_response.fallback_unit,
        units_for_slot.fallback_unit
    );
}

#[tokio::test]
#[ignore = "exists to print output for comparison"]
async fn get_sample_units_for_slot_output() {
    let logger = discard_logger();

    let server = MockServer::start().await;

    let market = MarketApi::new(
        (server.uri() + "/platform/")
            .parse()
            .expect("Wrong Market url"),
        &DEVELOPMENT,
        logger.clone(),
    )
    .expect("should create market instance");

    let categories: [&str; 3] = ["IAB3", "IAB13-7", "IAB5"];
    let rules = get_mock_rules(&categories);
    let campaign = mock_campaign(&rules);

    let platform_ad_units = AdUnitsResponse(campaign.ad_units.clone());
    let mock_slot = get_test_ad_slot(&rules, &categories);

    Mock::given(method("GET"))
        .and(path("/platform/units"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&platform_ad_units))
        .mount(&server)
        .await;

    let request = Request::post("/units-for-slot")
        .header(USER_AGENT, TEST_USER_AGENT)
        .header(CLOUDFLARE_IPCOUNTY_HEADER.clone(), TEST_CLOUDFLARE_IPCOUNTY)
        .body(
            serde_json::to_vec(&RequestBody {
                ad_slot: mock_slot,
                deposit_assets: Some(vec![campaign.channel.token].into_iter().collect()),
            })
            .expect("Should serialize"),
        )
        .unwrap();

    let actual_response = post_units_for_slot(request, &app)
        .await
        .expect("call shouldn't fail with provided data");

    assert_eq!(StatusCode::OK, actual_response.status());

    let units_for_slot: UnitsForSlotResponse =
        serde_json::from_slice(&hyper::body::to_bytes(actual_response).await.unwrap())
            .expect("Should deserialize");
    let units_for_slot_pretty =
        serde_json::to_string_pretty(&units_for_slot).expect("should turn to string");

    println!("{}", units_for_slot_pretty);
}
