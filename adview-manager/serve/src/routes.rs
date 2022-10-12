use std::sync::Arc;

use adex_primitives::{
    sentry::{units_for_slot, IMPRESSION},
    targeting::{input::Global, Input},
    test_util::{DUMMY_CAMPAIGN, DUMMY_IPFS, DUMMY_VALIDATOR_FOLLOWER, DUMMY_VALIDATOR_LEADER},
    ToHex,
};
use adview_manager::{
    get_unit_html_with_events, manager::Size, manager::DEFAULT_TOKENS, Manager, Options,
};
use axum::{response::Html, Extension};
use chrono::Utc;
use log::debug;
use wiremock::{
    matchers::{method, path, query_param},
    Mock, MockServer, ResponseTemplate,
};

use tera::Context;

use crate::app::State;

/// `GET /`
pub async fn get_index(Extension(state): Extension<Arc<State>>) -> Html<String> {
    let html = state
        .tera
        .render("index.html", &Default::default())
        .expect("Should render");

    Html(html)
}

/// `GET /preview/ad`
pub async fn get_preview_ad(Extension(state): Extension<Arc<State>>) -> Html<String> {
    // For mocking the `get_market_demand_resp` call
    let mock_server = MockServer::start().await;

    let market_url = mock_server.uri().parse().unwrap();
    let whitelisted_tokens = DEFAULT_TOKENS.clone();
    let disabled_video = false;
    let publisher_addr = "0x0000000000000000626f62627973686d75726461"
        .parse()
        .unwrap();

    let options = Options {
        market_url,
        market_slot: DUMMY_IPFS[0],
        publisher_addr,
        // All passed tokens must be of the same price and decimals, so that the amounts can be accurately compared
        whitelisted_tokens,
        size: Some(Size::new(300, 100)),
        // TODO: Check this value
        navigator_language: Some("bg".into()),
        /// Defaulted
        disabled_video,
        disabled_sticky: false,
        validators: vec![
            DUMMY_VALIDATOR_LEADER.clone(),
            DUMMY_VALIDATOR_FOLLOWER.clone(),
        ],
    };

    let manager =
        Manager::new(options.clone(), Default::default()).expect("Failed to create AdView Manager");
    let pub_prefix = publisher_addr.to_hex();

    let units_for_slot_resp = units_for_slot::response::Response {
        targeting_input_base: Input {
            ad_view: None,
            global: Global {
                ad_slot_id: options.market_slot,
                ad_slot_type: "".into(),
                publisher_id: publisher_addr,
                country: Some("Bulgaria".into()),
                event_type: IMPRESSION,
                seconds_since_epoch: Utc::now(),
                user_agent_os: None,
                user_agent_browser_family: None,
            },
            campaign: None,
            balances: None,
            ad_unit_id: None,
            ad_slot: None,
        },
        accepted_referrers: vec![],
        fallback_unit: None,
        campaigns: vec![],
    };

    // Mock the `get_market_demand_resp` call
    let mock_call = Mock::given(method("GET"))
        // &depositAsset={}&depositAsset={}
        .and(path(format!("units-for-slot/{}", options.market_slot)))
        // pubPrefix=HEX&depositAsset=0xASSET1&depositAsset=0xASSET2
        .and(query_param("pubPrefix", pub_prefix))
        .and(query_param(
            "depositAsset",
            "0x6B175474E89094C44Da98b954EedeAC495271d0F",
        ))
        // .and(query_param("depositAsset[]", "0x6B175474E89094C44Da98b954EedeAC495271d03"))
        .respond_with(ResponseTemplate::new(200).set_body_json(units_for_slot_resp))
        .expect(1)
        .named("get_market_demand_resp");

    // Mounting the mock on the mock server - it's now effective!
    mock_call.mount(&mock_server).await;

    let demand_resp = manager
        .get_market_demand_resp()
        .await
        .expect("Should return Mocked response");

    debug!("Mocked response: {demand_resp:?}");

    let supermarket_ad_unit = adex_primitives::sentry::units_for_slot::response::AdUnit {
        /// Same as `ipfs`
        ipfs: DUMMY_IPFS[1],
        media_url: "ipfs://QmcUVX7fvoLMM93uN2bD3wGTH8MXSxeL8hojYfL2Lhp7mR".to_string(),
        media_mime: "image/jpeg".to_string(),
        target_url: "https://www.adex.network/?stremio-test-banner-1".to_string(),
    };

    let code = get_unit_html_with_events(
        &options,
        &supermarket_ad_unit,
        "localhost",
        DUMMY_CAMPAIGN.id,
        &DUMMY_CAMPAIGN.validators,
        false,
    );

    let html = {
        let mut context = Context::new();
        context.insert("ad_code", &code);

        state
            .tera
            .render("ad.html", &context)
            .expect("Should render")
    };

    Html(html)
}

/// `GET /preview/video`
pub async fn get_preview_video(Extension(state): Extension<Arc<State>>) -> Html<String> {
    let whitelisted_tokens = DEFAULT_TOKENS.clone();
    let disabled_video = false;
    let publisher_addr = "0x0000000000000000626f62627973686d75726461"
        .parse()
        .unwrap();

    let options = Options {
        market_url: "http://placeholder.com".parse().unwrap(),
        market_slot: DUMMY_IPFS[0],
        publisher_addr,
        // All passed tokens must be of the same price and decimals, so that the amounts can be accurately compared
        whitelisted_tokens,
        size: Some(Size::new(728, 90)),
        // TODO: Check this value
        navigator_language: Some("bg".into()),
        /// Defaulted
        disabled_video,
        disabled_sticky: false,
        validators: vec![
            DUMMY_VALIDATOR_LEADER.clone(),
            DUMMY_VALIDATOR_FOLLOWER.clone(),
        ],
    };

    // legacy_728x90
    let video_ad_unit = adex_primitives::sentry::units_for_slot::response::AdUnit {
        /// Same as `ipfs`
        ipfs: "QmShJ6tsJ7LHLiYGX4EAmPyCPWJuCnvm6AKjweQPnw9qfh"
            .parse()
            .expect("Valid IPFS"),
        media_url: "ipfs://Qmevmms1AZgYXS27A9ghSjHn4DSaHMfgYcD2SoGW14RYGy".to_string(),
        media_mime: "video/mp4".to_string(),
        target_url: "https://www.stremio.com/downloads".to_string(),
    };

    let video_html = get_unit_html_with_events(
        &options,
        &video_ad_unit,
        "localhost",
        DUMMY_CAMPAIGN.id,
        &DUMMY_CAMPAIGN.validators,
        false,
    );

    // let video_html = get_unit_html_with_events(&options, );
    let html = {
        let mut context = Context::new();
        context.insert("ad_code", &video_html);

        state
            .tera
            .render("ad.html", &context)
            .expect("Should render")
    };

    Html(html)
}
