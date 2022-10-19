use std::sync::Arc;

use anyhow::{anyhow, bail};
use axum::{
    http::{header::ACCEPT_LANGUAGE, HeaderMap, StatusCode},
    response::Html,
    Extension, Json,
};
use chrono::Utc;
use tera::Context;
use tracing::debug;
use wiremock::{
    matchers::{method, path, query_param},
    Mock, MockServer, ResponseTemplate,
};

use adex_primitives::{
    config::GANACHE_CONFIG,
    sentry::{units_for_slot, IMPRESSION},
    targeting::{input::Global, Input},
    test_util::{
        DUMMY_CAMPAIGN, DUMMY_IPFS, DUMMY_VALIDATOR_FOLLOWER, DUMMY_VALIDATOR_LEADER, PUBLISHER,
    },
    util::ApiUrl,
    AdSlot, ToHex,
};
use adview_manager::{
    get_unit_html_with_events, manager::Size, manager::DEFAULT_TOKENS, Manager, Options,
};

use crate::app::{Error, State};

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
    // For mocking the `get_units_for_slot_resp` call
    let mock_server = MockServer::start().await;

    let whitelisted_tokens = DEFAULT_TOKENS.clone();
    let disabled_video = false;
    let publisher_addr = "0x0000000000000000626f62627973686d75726461"
        .parse()
        .unwrap();

    let options = Options {
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
            ApiUrl::parse(&DUMMY_VALIDATOR_LEADER.url).expect("should parse"),
            ApiUrl::parse(&DUMMY_VALIDATOR_FOLLOWER.url).expect("should parse"),
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

    // Mock the `get_units_for_slot_resp` call
    let mock_call = Mock::given(method("GET"))
        .and(path(format!("units-for-slot/{}", options.market_slot)))
        // pubPrefix=HEX&depositAssets[]=0xASSET1&depositAssets[]=0xASSET2
        .and(query_param("pubPrefix", pub_prefix))
        .and(query_param(
            "depositAssets[]",
            "0x6B175474E89094C44Da98b954EedeAC495271d0F",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_json(units_for_slot_resp))
        .expect(1)
        .named("get_units_for_slot_resp");

    // Mounting the mock on the mock server - it's now effective!
    mock_call.mount(&mock_server).await;

    let demand_resp = manager
        .get_units_for_slot_resp()
        .await
        .expect("Should return Mocked response");

    debug!("Mocked response: {demand_resp:?}");

    let ufs_ad_unit = adex_primitives::sentry::units_for_slot::response::AdUnit {
        /// Same as `ipfs`
        ipfs: DUMMY_IPFS[1],
        media_url: "ipfs://QmcUVX7fvoLMM93uN2bD3wGTH8MXSxeL8hojYfL2Lhp7mR".to_string(),
        media_mime: "image/jpeg".to_string(),
        target_url: "https://www.adex.network/?stremio-test-banner-1".to_string(),
    };

    let code = get_unit_html_with_events(
        &options,
        &ufs_ad_unit,
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
            ApiUrl::parse(&DUMMY_VALIDATOR_LEADER.url).expect("should parse"),
            ApiUrl::parse(&DUMMY_VALIDATOR_FOLLOWER.url).expect("should parse"),
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

/// `GET /preview/:slot`
// pub async fn get_slot_preview(
//     Extension(state): Extension<Arc<State>>,
//     Path(slot): Path<IPFS>,
//     headers: HeaderMap,
// ) -> Result<Html<String>, Error> {
//     let config = GANACHE_CONFIG.clone();

//     // extracted from Accept-language header
//     let navigator_language = headers
//         .get(ACCEPT_LANGUAGE)
//         .map(|value| value.to_str())
//         .transpose()?
//         .map(|s| parse_navigator_language(s))
//         .transpose()?
//         .flatten()
//         // TODO: make configurable?
//         .unwrap_or("en".into());

//     let whitelisted_tokens = config
//         .chains
//         .iter()
//         .map(|(_, chain)| chain.tokens.iter().map(|(_, token)| token.address))
//         .flatten()
//         .collect();

//         let options = Options {
//             market_slot: ad_slot.ipfs,
//             publisher_addr: *PUBLISHER,
//             // All passed tokens must be of the same price, so that the amounts can be accurately compared
//             whitelisted_tokens,
//             size: Some(
//                 size_from_type(&ad_slot.ad_type)
//                     .map_err(|error| Error::anyhow_status(error, StatusCode::BAD_REQUEST))?,
//             ),
//             navigator_language: Some(navigator_language),
//             /// Defaulted
//             disabled_video: false,
//             disabled_sticky: false,
//         };

//         let manager = Manager::new(options, Default::default())?;

//         let next_ad_unit = manager.get_next_ad_unit().await?;

//         let html = {
//             let mut context = Context::new();
//             context.insert("next_ad_unit", &next_ad_unit);
//             context.insert("ad_slot", &ad_slot);

//             state
//                 .tera
//                 .render("next_ad.html", &context)
//                 .expect("Should render")
//         };

//         Ok(Html(html))
// }

/// `POST /preview`
///
/// Uses the provided with the POST data [`AdSlot`] and get's a matching [`AdUnit`] html
/// with the manager.
///
/// Uses the Ganache config to select all the whitelisted tokens in all chains.
/// It's configured to use locally running sentry validators at ports `8005` and `8006`.
#[axum::debug_handler]
pub async fn post_slot_preview(
    Extension(state): Extension<Arc<State>>,
    Json(ad_slot): Json<AdSlot>,
    headers: HeaderMap,
) -> Result<Html<String>, Error> {
    let config = GANACHE_CONFIG.clone();

    // extracted from Accept-language header
    let navigator_language = headers
        .get(ACCEPT_LANGUAGE)
        .map(|value| value.to_str())
        .transpose()?
        .map(parse_navigator_language)
        .transpose()?
        .flatten()
        // TODO: make configurable?
        .unwrap_or_else(|| "en".into());

    let whitelisted_tokens = config
        .chains
        .iter()
        .flat_map(|(_, chain)| chain.tokens.iter().map(|(_, token)| token.address))
        .collect();

    let options = Options {
        market_slot: ad_slot.ipfs,
        publisher_addr: *PUBLISHER,
        // All passed tokens must be of the same price, so that the amounts can be accurately compared
        whitelisted_tokens,
        size: Some(
            size_from_type(&ad_slot.ad_type)
                .map_err(|error| Error::anyhow_status(error, StatusCode::BAD_REQUEST))?,
        ),
        navigator_language: Some(navigator_language),
        /// Defaulted
        disabled_video: false,
        disabled_sticky: false,
        validators: vec![
            ApiUrl::parse(&DUMMY_VALIDATOR_LEADER.url).expect("should parse"),
            ApiUrl::parse(&DUMMY_VALIDATOR_FOLLOWER.url).expect("should parse"),
        ],
    };

    let manager = Manager::new(options, Default::default())?;

    let next_ad_unit = manager.get_next_ad_unit().await?;

    let html = {
        let mut context = Context::new();
        context.insert("next_ad_unit", &next_ad_unit);
        context.insert("ad_slot", &ad_slot);

        state
            .tera
            .render("next_ad.html", &context)
            .expect("Should render")
    };

    Ok(Html(html))
}

/// It takes a type like `legacy_300x250` and returns a [`Size`] struct with size of `300x250`.
fn size_from_type(ad_type: &str) -> anyhow::Result<Size> {
    let size_str = ad_type
        .strip_prefix("legacy_")
        .ok_or_else(|| anyhow!("Missing `legacy_` prefix"))?;

    let (width_str, height_str) = size_str
        .split_once('x')
        .ok_or_else(|| anyhow!("Width and height should be separated by `x`"))?;

    match (width_str.parse::<u64>(), height_str.parse::<u64>()) {
        (Ok(width), Ok(height)) => Ok(Size::new(width, height)),
        (Ok(_), Err(_err)) => bail!("Height `{height_str}` failed to be parse as u64"),
        (Err(_err), Ok(_)) => bail!("Width `{width_str}` failed to be parse as u64"),
        (Err(_), Err(_)) => {
            bail!("Width `{width_str}` and Height `{height_str}` failed to be parse as u64")
        }
    }
}

/// This function is flawed because we don't parse the language itself and we
/// don't take the weights into account!
fn parse_navigator_language(accept_language: &str) -> anyhow::Result<Option<String>> {
    if accept_language == "*" {
        return Ok(None);
    }

    let first_language = accept_language
        .chars()
        .take_while(|ch| ch != &',' && ch != &';')
        .collect::<String>();

    if first_language.is_empty() {
        return Ok(None);
    }

    Ok(Some(first_language))
}

#[cfg(test)]
mod test {
    use adview_manager::manager::Size;

    use super::{parse_navigator_language, size_from_type};

    #[test]
    fn test_parse_navigator_language() {
        {
            let any_language = "*";
            let result = parse_navigator_language(any_language).expect("Should parse language");

            assert_eq!(None, result, "Any language should be matched");
        }

        {
            let result = parse_navigator_language("").expect("Should parse language");

            assert_eq!(None, result, "No language");
        }

        {
            let multiple_with_factor = "fr-CH, fr;q=0.9, en;q=0.8, de;q=0.7, *;q=0.5";
            let result =
                parse_navigator_language(multiple_with_factor).expect("Should parse language");

            assert_eq!(
                Some("fr-CH".to_string()),
                result,
                "fr-CH language should be matched"
            );
        }

        {
            let english_with_factor = "en-US,en;q=0.5";
            let result =
                parse_navigator_language(english_with_factor).expect("Should parse language");

            assert_eq!(
                Some("en-US".to_string()),
                result,
                "en-US language should be matched"
            );
        }

        {
            let multiple_without_factor = "en-US, zh-CN, ja-JP";
            let result =
                parse_navigator_language(multiple_without_factor).expect("Should parse language");

            assert_eq!(
                Some("en-US".to_string()),
                result,
                "en-US language should be matched"
            );
        }

        // Malformed
        // We don't have any validation, so we should get back the same value
        {
            let malformed = "ab-CDE-ffff l -- adsfasd '";
            let result = parse_navigator_language(malformed).expect("Should parse language");

            assert_eq!(
                Some(malformed.to_string()),
                result,
                "The malformed language should be matched"
            );
        }
    }

    #[test]
    fn test_size_from_type() {
        let legacy = [
            (Size::new(300, 250), "legacy_300x250"),
            (Size::new(250, 250), "legacy_250x250"),
            (Size::new(240, 400), "legacy_240x400"),
            (Size::new(336, 280), "legacy_336x280"),
            (Size::new(180, 150), "legacy_180x150"),
            (Size::new(300, 100), "legacy_300x100"),
            (Size::new(720, 300), "legacy_720x300"),
            (Size::new(468, 60), "legacy_468x60"),
            (Size::new(234, 60), "legacy_234x60"),
            (Size::new(88, 31), "legacy_88x31"),
            (Size::new(120, 90), "legacy_120x90"),
            (Size::new(120, 60), "legacy_120x60"),
            (Size::new(120, 240), "legacy_120x240"),
            (Size::new(125, 125), "legacy_125x125"),
            (Size::new(728, 90), "legacy_728x90"),
            (Size::new(160, 600), "legacy_160x600"),
            (Size::new(120, 600), "legacy_120x600"),
            (Size::new(300, 600), "legacy_300x600"),
        ];

        for (expected, type_string) in legacy {
            let actual_size = size_from_type(type_string).expect("Should parse Ad type");
            pretty_assertions::assert_eq!(
                expected,
                actual_size,
                "Expected Size does not match with the parsed one"
            );
        }
    }
}
