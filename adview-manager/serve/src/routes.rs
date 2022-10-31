use std::{collections::HashSet, fmt::Display, ops::Deref, str::FromStr, sync::Arc};

use anyhow::{anyhow, bail};
use axum::{
    http::{header::ACCEPT_LANGUAGE, HeaderMap, StatusCode},
    response::Html,
    Extension,
};
use axum_extra::extract::Form;
use chrono::{TimeZone, Utc};
use once_cell::sync::Lazy;
use reqwest::Client;
use serde::{de::Error as _, Deserialize, Deserializer, Serialize};
use tera::Context;

use adex_primitives::{
    config::GANACHE_CONFIG,
    platform::{AdSlotResponse, Website},
    targeting::Rules,
    test_util::{
        DUMMY_AD_UNITS, DUMMY_CAMPAIGN, DUMMY_IPFS, DUMMY_VALIDATOR_FOLLOWER,
        DUMMY_VALIDATOR_LEADER, IDS, PUBLISHER,
    },
    util::ApiUrl,
    AdSlot, Address,
};
use adview_manager::{get_unit_html_with_events, manager::Size, Manager, Options};

use crate::app::{Error, State};

/// All the configured tokens in the `ganache.toml` config file
pub static WHITELISTED_TOKENS: Lazy<HashSet<Address>> = Lazy::new(|| {
    GANACHE_CONFIG
        .chains
        .values()
        .flat_map(|chain| chain.tokens.values().map(|token| token.address))
        .collect()
});

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
    let disabled_video = false;
    let publisher_addr = *PUBLISHER;
    let campaign = DUMMY_CAMPAIGN.clone();
    // ordering matters
    let validators_url = vec![
        ApiUrl::parse(&campaign.leader().unwrap().url).expect("should parse"),
        ApiUrl::parse(&campaign.leader().unwrap().url).expect("should parse"),
    ];

    let options = Options {
        market_slot: DUMMY_IPFS[0],
        publisher_addr,
        // All passed tokens must be of the same price and decimals, so that the amounts can be accurately compared
        whitelisted_tokens: WHITELISTED_TOKENS.clone(),
        size: Some(Size::new(300, 100)),
        navigator_language: Some("bg".into()),
        /// Defaulted
        disabled_video,
        disabled_sticky: false,
        validators: validators_url,
    };

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
        campaign.id,
        &campaign.validators,
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
    let disabled_video = false;
    let publisher_addr = *PUBLISHER;

    let options = Options {
        market_slot: DUMMY_IPFS[0],
        publisher_addr,
        // All passed tokens must be of the same price and decimals, so that the amounts can be accurately compared
        whitelisted_tokens: WHITELISTED_TOKENS.clone(),
        size: Some(Size::new(728, 90)),
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdSlotPreview {
    #[serde(with = "form_json")]
    adslot_response: AdSlotResponse,
    #[serde(default)]
    disabled_video: bool,
    #[serde(default)]
    disabled_sticky: bool,
    publisher: Address,
    #[serde(deserialize_with = "empty_field_string::<_, ApiUrl>")]
    validators: Vec<ApiUrl>,
    #[serde(deserialize_with = "empty_field_string::<_, Address>")]
    whitelisted_tokens: Vec<Address>,
}

mod form_json {
    use serde::{
        de::{DeserializeOwned, Error as _},
        ser::Error as _,
        Deserialize, Deserializer, Serialize, Serializer,
    };

    pub fn serialize<S, T>(value: &T, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
        T: Serialize,
    {
        let json = serde_json::to_string(value).map_err(S::Error::custom)?;
        serializer.serialize_str(&json)
    }

    pub fn deserialize<'de, D, T>(deserializer: D) -> Result<T, D::Error>
    where
        D: Deserializer<'de>,
        T: DeserializeOwned,
    {
        let json = String::deserialize(deserializer)?;

        serde_json::from_str::<T>(&json).map_err(D::Error::custom)
    }
}

pub fn empty_field_string<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: FromStr,
    <T as FromStr>::Err: Display,
{
    let vec_of_string = <Vec<String>>::deserialize(deserializer)?;

    vec_of_string
        .into_iter()
        .filter_map(|string| {
            if string.is_empty() {
                None
            } else {
                Some(string.parse::<T>())
            }
        })
        .collect::<Result<_, _>>()
        .map_err(D::Error::custom)
}

/// `GET /preview`
///
/// Shows a form to submit a JSON [`AdSlot`] Response ([`AdSlotResponse`])
/// from the platform and preview the html with the matching of an [`AdUnit`]
/// using the manager.
///
/// Uses the Ganache config to select all the whitelisted tokens in all chains.
/// It's configured to use locally running sentry validators at ports `8005` and `8006`.
/// Alongside locally running mocked platform at `8004`.
///
/// [`AdUnit`]: adex_primitives::AdUnit
#[axum::debug_handler]
pub async fn get_slot_preview_form(
    Extension(state): Extension<Arc<State>>,
) -> Result<Html<String>, Error> {
    // let config = GANACHE_CONFIG.clone();

    let validators = vec![
        ApiUrl::parse(&DUMMY_VALIDATOR_LEADER.url).expect("should parse"),
        ApiUrl::parse(&DUMMY_VALIDATOR_FOLLOWER.url).expect("should parse"),
    ];

    let ad_slot = AdSlot {
        ipfs: DUMMY_IPFS[0],
        ad_type: "legacy_300x100".to_string(),
        min_per_impression: None,
        rules: Rules::default(),
        fallback_unit: Some(DUMMY_AD_UNITS[0].ipfs),
        owner: IDS[&PUBLISHER],
        created: Utc.ymd(2019, 7, 29).and_hms(7, 0, 0),
        title: Some("Test slot 1".to_string()),
        description: Some("Test slot for running integration tests".to_string()),
        website: Some("https://adex.network".to_string()),
        archived: false,
        modified: Some(Utc.ymd(2019, 7, 29).and_hms(7, 0, 0)),
    };

    let adslot_response = AdSlotResponse {
        slot: ad_slot,
        fallback: Some(DUMMY_AD_UNITS[0].clone()),
        website: Some(Website {
            categories: vec![
                "IAB3".to_string(),
                "IAB13-7".to_string(),
                "IAB5".to_string(),
            ],
            accepted_referrers: vec![],
        }),
    };

    let html = {
        let mut context = Context::new();
        context.insert("default_publisher", &*PUBLISHER);
        context.insert("default_validators", &validators);
        context.insert("default_adslot_response", &adslot_response);
        context.insert("default_whitelisted_tokens", WHITELISTED_TOKENS.deref());

        state
            .tera
            .render("preview_form.html", &context)
            .expect("Should render")
    };

    Ok(Html(html))
}

/// `POST /preview`
///
/// Uses the provided with the POST data and gets a matching [`AdUnit`] html
/// with the manager.
///
/// Uses the Ganache config to select all the whitelisted tokens in all chains.
/// It's configured to use locally running sentry validators at ports `8005` and `8006`.
/// Alongside locally running mocked platform at `8004`.
///
/// [`AdUnit`]: adex_primitives::AdUnit
#[axum::debug_handler]
pub async fn post_slot_preview(
    Extension(state): Extension<Arc<State>>,
    headers: HeaderMap,
    Form(adslot_preview): Form<AdSlotPreview>,
) -> Result<Html<String>, Error> {
    let ad_slot = adslot_preview.adslot_response.slot.clone();

    // setup the `AdSlotResponse` from the Platform
    setup_platform_response(&adslot_preview.adslot_response).await?;

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

    let options = Options {
        market_slot: ad_slot.ipfs,
        publisher_addr: *PUBLISHER,
        // All passed tokens must be of the same price, so that the amounts can be accurately compared
        whitelisted_tokens: adslot_preview.whitelisted_tokens.into_iter().collect(),
        size: Some(
            size_from_type(&ad_slot.ad_type)
                .map_err(|error| Error::anyhow_status(error, StatusCode::BAD_REQUEST))?,
        ),
        navigator_language: Some(navigator_language),
        /// Defaulted
        disabled_video: adslot_preview.disabled_video,
        disabled_sticky: adslot_preview.disabled_sticky,
        validators: adslot_preview.validators,
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

async fn setup_platform_response(response: &AdSlotResponse) -> anyhow::Result<()> {
    let client = Client::builder().build()?;

    client
        .post("http://localhost:8004/slot")
        .json(&response)
        .send()
        .await?
        .error_for_status()?;

    Ok(())
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
