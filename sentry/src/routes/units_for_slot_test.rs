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
    targeting::Rules,
    test_util::{CAMPAIGNS, DUMMY_AD_UNITS, DUMMY_IPFS, IDS, PUBLISHER},
    unified_num::FromWhole,
    util::logging::new_logger,
    AdSlot, AdUnit,
};

use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

use super::*;
use crate::{
    db::update_campaign,
    routes::campaign::create_campaign,
    test_util::{setup_dummy_app, ApplicationGuard},
    Auth,
};

// User Agent OS: Linux (only in `woothee`)
// User Agent Browser Family: Firefox
const LINUX_FIREFOX_USER_AGENT: &str =
    "Mozilla/5.0 (X11; Ubuntu; Linux x86_64; rv:83.0) Gecko/20100101 Firefox/83.0";
// uses two-letter country codes: https://en.wikipedia.org/wiki/ISO_3166-1_alpha-2
const BG_CLOUDFLARE_IPCOUNTRY: &str = "BG";

/// With the fallback AdUnit included in the returned data.
const TEST_AD_SLOT: Lazy<(AdSlot, AdUnit)> = Lazy::new(|| {
    let fallback_unit = DUMMY_AD_UNITS[0].clone();

    let ad_slot = AdSlot {
        ipfs: DUMMY_IPFS[0],
        ad_type: "legacy_250x250".to_string(),
        archived: false,
        created: Utc.ymd(2019, 7, 29).and_hms(7, 0, 0),
        description: Some("Test slot for running integration tests".to_string()),
        fallback_unit: Some(fallback_unit.ipfs),
        min_per_impression: Some(
            [
                (
                    GANACHE_INFO_1.tokens["Mocked TOKEN 1"].address,
                    UnifiedNum::from_whole(0.010),
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

    assert_ne!(
        fallback_unit.ipfs, ad_slot.ipfs,
        "The test AdSlot & Fallback AdUnit should have different IPFS"
    );

    (ad_slot, fallback_unit)
});

async fn setup_mocked_platform_dummy_app() -> (MockServer, ApplicationGuard) {
    // For mocking the `get_market_demand_resp` call
    let mock_server = MockServer::start().await;

    let platform_url = mock_server.uri().parse().unwrap();

    let mut app_guard = setup_dummy_app().await;
    app_guard.app.logger = new_logger("sentry-dummy-app");
    debug!(
        &app_guard.app.logger,
        "With platform mocker server at {}", &platform_url
    );

    app_guard.app.platform_api.platform_url = platform_url;

    (mock_server, app_guard)
}

#[tokio::test]
async fn test_targeting_input() {
    let (mock_server, app_guard) = setup_mocked_platform_dummy_app().await;
    let app = Arc::new(app_guard.app);

    let (ad_slot, fallback_unit) = TEST_AD_SLOT.clone();

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

            CAMPAIGNS[0].clone()
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

            CAMPAIGNS[1].clone()
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

            CAMPAIGNS[2].clone()
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

    Mock::given(method("GET"))
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

    let expected_targeting_input = Input {
        ad_view: None,
        global: input::Global {
            ad_slot_id: DUMMY_IPFS[0],
            ad_slot_type: "legacy_250x250".to_string(),
            publisher_id: *PUBLISHER,
            country: Some(BG_CLOUDFLARE_IPCOUNTRY.to_string()),
            event_type: IMPRESSION,
            // we can't know only the timestamp
            seconds_since_epoch: response
                .targeting_input_base
                .global
                .seconds_since_epoch
                .clone(),
            user_agent_os: Some("Linux".to_string()),
            user_agent_browser_family: Some("Firefox".to_string()),
        },
        // no AdUnit should be present
        ad_unit_id: None,
        // no balances
        balances: None,
        // no campaign
        campaign: None,
        ad_slot: Some(input::AdSlot {
            categories: vec!["IAB3".into(), "IAB13-7".into(), "IAB5".into()],
            hostname: "adex.network".to_string(),
        }),
    };

    debug!(&app.logger, "{:?}", response.targeting_input_base);
    assert_eq!(response.targeting_input_base, expected_targeting_input);

    assert!(response.accepted_referrers.is_empty());
    let expected_fallback_unit = ResponseAdUnit {
        ipfs: fallback_unit.ipfs,
        media_url: fallback_unit.media_url,
        media_mime: fallback_unit.media_mime,
        target_url: fallback_unit.target_url,
    };
    assert_eq!(response.fallback_unit, Some(expected_fallback_unit));

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
async fn test_inactive_campaign() {
    let (mock_server, app_guard) = setup_mocked_platform_dummy_app().await;
    let app = Arc::new(app_guard.app);

    let (ad_slot, fallback_unit) = TEST_AD_SLOT.clone();

    // campaign creation
    // Instead of inserting manually redis & postgres data for the campaign, just use the route
    // to create the campaigns for the test in the DB,
    // then manually update the campaign with an earlier `Active.to`
    //
    let _inactive_campaign = {
        let active_campaign = CAMPAIGNS[2].clone();

        let deposit = Deposit {
            total: UnifiedNum::from_whole(100_000_000)
                .to_precision(CAMPAIGNS[2].token.precision.into()),
        };

        app.adapter.client.set_deposit(
            &active_campaign.of_channel(),
            active_campaign.context.creator,
            deposit,
        );

        let created_campaign = create_campaign(
            Json(CreateCampaign::from_campaign(
                active_campaign.context.clone(),
            )),
            Extension(Auth {
                era: 0,
                uid: active_campaign.context.creator.into(),
                chain: active_campaign.chain.clone(),
            }),
            Extension(app.clone()),
        )
        .await
        .expect("Should create campaign");
        assert_eq!(&created_campaign.0, &active_campaign.context);

        let mut inactive_campaign = active_campaign;
        inactive_campaign.context.active.to = Utc.ymd(2022, 10, 5).and_hms(0, 0, 0);

        // manually update the campaign to have an earlier Active.to
        update_campaign(&app.pool, &inactive_campaign.context)
            .await
            .expect("Should update Campaign to inactive (Active.to < now)");

        inactive_campaign
    };

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

    Mock::given(method("GET"))
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

    assert_eq!(
        Vec::<response::Campaign>::new(),
        response.campaigns,
        "Campaign should not be matched because it's inactive"
    );
}

#[tokio::test]
async fn test_creator_is_not_the_provided_publisher() {
    let (mock_server, app_guard) = setup_mocked_platform_dummy_app().await;
    let app = Arc::new(app_guard.app);

    // we only match campaign 1 but not campaign 0 because the `creator == publisher` (`AdSlot.owner` in this case)
    let (campaign_0, campaign_1) = {
        let campaign_0 = {
            let mut campaign = CAMPAIGNS[0].clone();
            // set the creator to PUBLISHER
            campaign.context.creator = *PUBLISHER;

            let deposit = Deposit {
                total: UnifiedNum::from_whole(200_000)
                    .to_precision(campaign.token.precision.into()),
            };

            app.adapter.client.set_deposit(
                &campaign.of_channel(),
                campaign.context.creator,
                deposit,
            );

            // campaign creation
            // Instead of inserting manually redis & postgres data for the campaign, just use the route
            // to create the campaigns for the test in the DB
            {
                let created_campaign = create_campaign(
                    // erase the ID of the campaign and generate a new ID randomly
                    Json(CreateCampaign::from_campaign_erased(
                        campaign.context.clone(),
                        None,
                    )),
                    Extension(Auth {
                        era: 0,
                        uid: campaign.context.creator.into(),
                        chain: campaign.chain.clone(),
                    }),
                    Extension(app.clone()),
                )
                .await
                .expect("Should create campaign");
                assert_ne!(
                    &created_campaign.0.id, &campaign.context.id,
                    "Create campaign should have different ID"
                );
            }

            campaign
        };

        // do not change anything about this campaign as it's going to be matched against.
        let campaign_1 = {
            let campaign = CAMPAIGNS[1].clone();

            let deposit = Deposit {
                total: UnifiedNum::from_whole(200_000)
                    .to_precision(campaign.token.precision.into()),
            };

            app.adapter.client.set_deposit(
                &campaign.of_channel(),
                campaign.context.creator,
                deposit,
            );

            // campaign creation
            // Instead of inserting manually redis & postgres data for the campaign, just use the route
            // to create the campaigns for the test in the DB
            {
                let created_campaign = create_campaign(
                    // use the same CampaignId of the Campaign
                    Json(CreateCampaign::from_campaign(campaign.context.clone())),
                    Extension(Auth {
                        era: 0,
                        uid: campaign.context.creator.into(),
                        chain: campaign.chain.clone(),
                    }),
                    Extension(app.clone()),
                )
                .await
                .expect("Should create campaign");
                assert_eq!(
                    &created_campaign.0, &campaign.context,
                    "We want to keep the same campaign and it's id"
                );
            }

            campaign
        };

        (campaign_0, campaign_1)
    };

    // set the AdSlot min_per_impression to match the 2 campaigns
    let (ad_slot, fallback_unit) = {
        let (mut ad_slot, fallback_unit) = TEST_AD_SLOT.clone();

        let slot_min_impression = ad_slot
            .min_per_impression
            .as_mut()
            .expect("Should have min_per_impression set");

        let min_per_impression = UnifiedNum::from_whole(0.0003);
        assert_eq!(
            campaign_0.token, campaign_1.token,
            "Both campaigns should have the same token to apply the min_per_impression on"
        );
        assert!(
            min_per_impression < campaign_0.context.pricing_bounds[&IMPRESSION].min,
            "AdSlot.min_per_impression should be less than Campaign 0 pricing bound in order to match it"
        );
        assert!(
            min_per_impression < campaign_1.context.pricing_bounds[&IMPRESSION].min,
            "AdSlot.min_per_impression should be less than Campaign 1 pricing bound in order to match it"
        );
        slot_min_impression.insert(campaign_0.token.address, min_per_impression);

        (ad_slot, fallback_unit)
    };

    let platform_response = AdSlotResponse {
        slot: ad_slot.clone(),
        fallback: Some(fallback_unit.clone()),
        website: Some(Website {
            categories: vec![
                "IAB3".to_string(),
                "IAB13-7".to_string(),
                "IAB5".to_string(),
            ],
            accepted_referrers: vec![],
        }),
    };

    let headers = [(
        CLOUDFLARE_IPCOUNTRY_HEADER.clone(),
        BG_CLOUDFLARE_IPCOUNTRY
            .parse::<HeaderValue>()
            .expect("Should parse header value"),
    )]
    .into_iter()
    .collect();

    Mock::given(method("GET"))
        .and(path(format!("/slot/{}", ad_slot.ipfs)))
        .respond_with(ResponseTemplate::new(200).set_body_json(platform_response))
        .expect(1)
        .named("platform_slot")
        .mount(&mock_server)
        .await;

    let response = get_units_for_slot(
        Extension(app.clone()),
        Path(ad_slot.ipfs),
        Qs(Query::default()),
        Some(TypedHeader(UserAgent::from_static(
            LINUX_FIREFOX_USER_AGENT,
        ))),
        headers,
    )
    .await
    .expect("Should return response")
    .0;

    assert_eq!(
        *PUBLISHER,
        response.targeting_input_base.global.publisher_id
    );

    let expected_fallback_unit = ResponseAdUnit {
        ipfs: fallback_unit.ipfs,
        media_url: fallback_unit.media_url,
        media_mime: fallback_unit.media_mime,
        target_url: fallback_unit.target_url,
    };
    assert_eq!(response.fallback_unit, Some(expected_fallback_unit));

    let expected_campaigns = vec![ResponseCampaign {
        campaign: campaign_1.context.clone(),
        units_with_price: vec![
            UnitsWithPrice {
                unit: (&campaign_1.context.ad_units[0]).into(),
                price: campaign_1.context.pricing_bounds[&IMPRESSION].min,
            },
            UnitsWithPrice {
                unit: (&campaign_1.context.ad_units[1]).into(),
                price: campaign_1.context.pricing_bounds[&IMPRESSION].min,
            },
        ],
    }];
    assert_eq!(response.campaigns, expected_campaigns);
}

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
