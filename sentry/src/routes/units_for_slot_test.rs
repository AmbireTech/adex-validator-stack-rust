use std::iter::Iterator;

use adapter::{
    ethereum::test_util::{GANACHE_INFO_1, GANACHE_INFO_1337},
    primitives::Deposit,
};
use axum::http::HeaderValue;
use chrono::{TimeZone, Utc};
use primitives::{
    platform::{AdSlotResponse, Website},
    sentry::{
        campaign_create::CreateCampaign,
        units_for_slot::response::{
            AdUnit as ResponseAdUnit, Campaign as ResponseCampaign, UnitsWithPrice,
        },
    },
    targeting::{Function, Rules, Value},
    test_util::{CAMPAIGNS, DUMMY_AD_UNITS, DUMMY_IPFS, IDS, PUBLISHER},
    unified_num::FromWhole,
    AdSlot,
};

use super::*;
use crate::{
    routes::campaign::create_campaign,
    test_util::{setup_dummy_app, ApplicationGuard},
    Auth,
};

// use url::Url;
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

// User Agent OS: Linux (only in `woothee`)
// User Agent Browser Family: Firefox
const LINUX_FIREFOX_USER_AGENT: &str =
    "Mozilla/5.0 (X11; Ubuntu; Linux x86_64; rv:83.0) Gecko/20100101 Firefox/83.0";
// uses two-letter country codes: https://en.wikipedia.org/wiki/ISO_3166-1_alpha-2
const BG_CLOUDFLARE_IPCOUNTRY: &str = "BG";

fn get_categories_rules(categories: &[&str]) -> Rules {
    let get_rule = Function::new_get("adSlot.categories");
    let categories_array = Value::Array(categories.iter().map(|s| Value::new_string(s)).collect());
    let intersects_rule = Function::new_intersects(get_rule, categories_array);

    Rules(vec![Function::new_only_show_if(intersects_rule).into()])
}

async fn setup_mocked_platform_dummy_app() -> (MockServer, ApplicationGuard) {
    // For mocking the `get_market_demand_resp` call
    let mock_server = MockServer::start().await;

    let platform_url = mock_server.uri().parse().unwrap();

    let mut app_guard = setup_dummy_app().await;
    app_guard.app.platform_api.platform_url = platform_url;

    (mock_server, app_guard)
}

#[tokio::test]
async fn test_targeting_input() {
    let (mock_server, app_guard) = setup_mocked_platform_dummy_app().await;
    let app = Arc::new(app_guard.app);

    let fallback_unit = DUMMY_AD_UNITS[0].clone();

    let ad_slot = AdSlot {
        ipfs: DUMMY_IPFS[0],
        ad_type: "legacy_250x250".to_string(),
        archived: false,
        created: Utc.ymd(2019, 7, 29).and_hms(7, 0, 0),
        description: Some("Test slot for running integration tests".to_string()),
        fallback_unit: None,
        min_per_impression: Some(
            [
                (
                    GANACHE_INFO_1.tokens["Mocked TOKEN 1"].address,
                    UnifiedNum::from_whole(0.0007),
                ),
                (
                    GANACHE_INFO_1337.tokens["Mocked TOKEN 1337"].address,
                    UnifiedNum::from_whole(0.001),
                ),
            ]
            .into_iter()
            .collect(),
        ),
        modified: Some(Utc.ymd(2019, 7, 29).and_hms(7, 0, 0)),
        owner: IDS[&PUBLISHER],
        title: Some("Test slot 1".to_string()),
        website: Some("https://adex.network".to_string()),
        rules: Rules::default(),
    };
    assert_ne!(fallback_unit.ipfs, ad_slot.ipfs);

    // we only match campaign 2 from Chain id 1 due to it's impression min price
    let campaigns = {
        let campaign_0 = {
            let deposit = Deposit {
                total: UnifiedNum::from_whole(200_000)
                    .to_precision(CAMPAIGNS[0].token.precision.into()),
            };

            app.adapter.client.set_deposit(
                &CAMPAIGNS[0].of_channel(),
                CAMPAIGNS[0].context.creator,
                deposit,
            );

            CAMPAIGNS[0] /* .context */
                .clone()
        };

        let campaign_1 = {
            let deposit = Deposit {
                total: UnifiedNum::from_whole(200_000)
                    .to_precision(CAMPAIGNS[1].token.precision.into()),
            };

            app.adapter.client.set_deposit(
                &CAMPAIGNS[1].of_channel(),
                CAMPAIGNS[1].context.creator,
                deposit,
            );

            CAMPAIGNS[1] /* .context */
                .clone()
        };

        let matching_campaign_2 = {
            let deposit = Deposit {
                total: UnifiedNum::from_whole(100_000_000)
                    .to_precision(CAMPAIGNS[2].token.precision.into()),
            };

            app.adapter.client.set_deposit(
                &CAMPAIGNS[2].of_channel(),
                CAMPAIGNS[2].context.creator,
                deposit,
            );

            let min_impression_price =
                Function::new_only_show_if(Function::new_get("eventMinPrice"));

            let mut campaign_2 = CAMPAIGNS[2] /* .context */
                .clone();
            campaign_2.context.targeting_rules = Rules(vec![min_impression_price.into()]);

            campaign_2
        };

        [campaign_0, campaign_1, matching_campaign_2]
    };

    // campaign creation
    // Instead of inserting manually redis & postgres data for the campaign, just use the route
    // to create the campaigns for the test in the DB
    {
        // campaign 0
        {
            let created_campaign = create_campaign(
                Json(CreateCampaign::from_campaign(campaigns[0].context.clone())),
                Extension(Auth {
                    era: 0,
                    uid: campaigns[0].context.creator.into(),
                    chain: campaigns[0].chain.clone(),
                }),
                Extension(app.clone()),
            )
            .await
            .expect("Should create campaign");
            assert_eq!(&created_campaign.0, &campaigns[0].context);
        }

        // campaign 1
        {
            let created_campaign = create_campaign(
                Json(CreateCampaign::from_campaign(campaigns[1].context.clone())),
                Extension(Auth {
                    era: 0,
                    uid: campaigns[1].context.creator.into(),
                    chain: campaigns[1].chain.clone(),
                }),
                Extension(app.clone()),
            )
            .await
            .expect("Should create campaign");
            assert_eq!(&created_campaign.0, &campaigns[1].context);
        }

        // matching campaign 2
        {
            let created_campaign = create_campaign(
                Json(CreateCampaign::from_campaign(campaigns[2].context.clone())),
                Extension(Auth {
                    era: 0,
                    uid: campaigns[2].context.creator.into(),
                    chain: campaigns[2].chain.clone(),
                }),
                Extension(app.clone()),
            )
            .await
            .expect("Should create campaign");
            assert_eq!(&created_campaign.0, &campaigns[2].context);
        }
    }

    let platform_response = AdSlotResponse {
        slot: ad_slot.clone(),
        fallback: Some(fallback_unit.clone()),
        website: Some(Website {
            categories: [
                "IAB3".to_string(),
                "IAB13-7".to_string(),
                "IAB5".to_string(),
            ]
            .into_iter()
            .collect(),
            accepted_referrers: vec![],
        }),
    };

    let deposit_assets = [
        GANACHE_INFO_1.tokens["Mocked TOKEN 1"].address,
        GANACHE_INFO_1337.tokens["Mocked TOKEN 1337"].address,
    ]
    .into_iter()
    .collect();

    let headers = [(
        CLOUDFLARE_IPCOUNTRY_HEADER.clone(),
        BG_CLOUDFLARE_IPCOUNTRY
            .parse::<HeaderValue>()
            .expect("Should parse header value"),
    )]
    .into_iter()
    .collect();

    let _mock_guard = Mock::given(method("GET"))
        .and(path(format!("/slot/{}", ad_slot.ipfs)))
        .respond_with(ResponseTemplate::new(200).set_body_json(platform_response))
        .expect(1)
        .named("platform_slot")
        .mount(&mock_server)
        .await;

    let response = get_units_for_slot(
        Extension(app.clone()),
        Path(ad_slot.ipfs),
        Qs(Query { deposit_assets }),
        Some(TypedHeader(UserAgent::from_static(
            LINUX_FIREFOX_USER_AGENT,
        ))),
        headers,
    )
    .await
    .expect("Should return response")
    .0;

    // pub targeting_input_base: Input,
    // assert_eq!(response.targeting_input_base);
    assert!(response.accepted_referrers.is_empty());
    let expected_fallback_until = ResponseAdUnit {
        ipfs: fallback_unit.ipfs,
        media_url: fallback_unit.media_url,
        media_mime: fallback_unit.media_mime,
        target_url: fallback_unit.target_url,
    };
    assert_eq!(response.fallback_unit, Some(expected_fallback_until));

    let expected_campaigns = vec![ResponseCampaign {
        campaign: campaigns[2].context.clone(),
        units_with_price: vec![
            UnitsWithPrice {
                unit: (&campaigns[2].context.ad_units[0]).into(),
                price: campaigns[2].context.pricing_bounds[&IMPRESSION].min,
            },
            UnitsWithPrice {
                unit: (&campaigns[2].context.ad_units[1]).into(),
                price: campaigns[2].context.pricing_bounds[&IMPRESSION].min,
            },
        ],
    }];
    assert_eq!(response.campaigns, expected_campaigns);
}

#[tokio::test]
async fn test_non_active_campaign() {}

#[tokio::test]
async fn test_creator_is_publisher() {}

#[tokio::test]
async fn test_no_ad_units() {}

#[tokio::test]
async fn test_price_less_than_min_per_impression() {}

#[tokio::test]
async fn test_non_matching_deposit_asset() {}

#[tokio::test]
async fn test_multiple_campaigns() {}

#[tokio::test]
#[ignore = "exists to print output for comparison"]
async fn get_sample_units_for_slot_output() {}
