use std::ops::{Add, Mul};

use adex_primitives::{
    campaign::Validators,
    sentry::{
        units_for_slot::response::AdUnit, Event, EventType, InsertEventsRequest, CLICK, IMPRESSION,
    },
    BigNum, CampaignId,
};
use num_integer::Integer;

use crate::{
    manager::{Options, Size},
    WAIT_FOR_IMPRESSION,
};

const IPFS_GATEWAY: &str = "https://ipfs.moonicorn.network/ipfs/";

fn normalize_url(url: &str) -> String {
    if url.starts_with("ipfs://") {
        url.replacen("ipfs://", IPFS_GATEWAY, 1)
    } else {
        url.to_string()
    }
}

fn image_html(on_load: &str, size: Option<Size>, image_url: &str) -> String {
    let size = size
        .map(|Size { width, height }| format!("width=\"{width}\" height=\"{height}\""))
        .unwrap_or_default();

    format!("<img loading=\"lazy\" src=\"{image_url}\" alt=\"AdEx ad\" rel=\"nofollow\" onload=\"{on_load}\" {size}>")
}

fn video_html(on_load: &str, size: Option<Size>, image_url: &str, media_mime: &str) -> String {
    let size = size
        .map(|Size { width, height }| format!("width=\"{width}\" height=\"{height}\""))
        .unwrap_or_default();

    format!(
        "<video {size} loop autoplay onloadeddata=\"{on_load}\" muted>
            <source src=\"{image_url}\" type=\"{media_mime}\">
        </video>",
    )
}

fn adex_icon() -> &'static str {
    r#"<a href="https://www.adex.network" target="_blank" rel="noopener noreferrer"
            style="position: absolute; top: 0; right: 0;"
        >
            <svg version="1.1" xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink" x="0px" y="0px" width="18px"
                height="18px" viewBox="0 0 18 18" style="enable-background:new 0 0 18 18;" xml:space="preserve">
                <style type="text/css">
                    .st0{fill:#FFFFFF;}
                    .st1{fill:#1B75BC;}
                </style>
                <defs>
                </defs>
                <rect class="st0" width="18" height="18"/>
                <path class="st1" d="M14,12.1L10.9,9L14,5.9L12.1,4L9,7.1L5.9,4L4,5.9L7.1,9L4,12.1L5.9,14L9,10.9l3.1,3.1L14,12.1z M7.9,2L6.4,3.5
                    L7.9,5L9,3.9L10.1,5l1.5-1.5L10,1.9l-1-1L7.9,2 M7.9,16l-1.5-1.5L7.9,13L9,14.1l1.1-1.1l1.5,1.5L10,16.1l-1,1L7.9,16"/>
            </svg>
        </a>"#
}

pub(crate) fn is_video(ad_unit: &AdUnit) -> bool {
    ad_unit.media_mime.split('/').next() == Some("video")
}

/// Does not copy the JS impl, instead it generates the BigNum from the IPFS CID bytes
pub(crate) fn randomized_sort_pos(ad_unit: &AdUnit, seed: BigNum) -> BigNum {
    let bytes = ad_unit.ipfs.0.to_bytes();

    let unit_id = BigNum::from_bytes_be(&bytes);

    let x: BigNum = unit_id.mul(seed).add(BigNum::from(12345));

    x.mod_floor(&BigNum::from(0x80000000))
}

/// Generates the AdUnit HTML for a given ad
pub(crate) fn get_unit_html(
    size: Option<Size>,
    ad_unit: &AdUnit,
    hostname: &str,
    on_load: &str,
    on_click: &str,
) -> String {
    // replace all `"` quotes with a single quote `'`
    // these values are used inside `onclick` & `onload` html attributes
    let on_load = on_load.replace('\"', "'");
    let on_click = on_click.replace('\"', "'");
    let image_url = normalize_url(&ad_unit.media_url);

    let element_html = if is_video(ad_unit) {
        video_html(&on_load, size, &image_url, &ad_unit.media_mime)
    } else {
        image_html(&on_load, size, &image_url)
    };

    // @TODO click protection page
    let final_target_url = ad_unit.target_url.replace(
        "utm_source=adex_PUBHOSTNAME",
        &format!("utm_source=AdEx+({hostname})", hostname = hostname),
    );

    let max_min_size = size
        .map(|Size { width, height }| {
            format!(
                "max-width: {width}px; min-width: {min_width}px; height: {height}px;",
                // u64 / 2 will floor the result!
                min_width = width / 2
            )
        })
        .unwrap_or_default();

    format!("<div style=\"position: relative; overflow: hidden; {style}\">
        <a href=\"{final_target_url}\" target=\"_blank\" onclick=\"{on_click}\" rel=\"noopener noreferrer\">
        {element_html}
        </a>
        {adex_icon}
        </div>", style=max_min_size, adex_icon=adex_icon())
}

/// Generates the HTML for showing an Ad ([`AdUnit`]), as well as, the code for sending the events.
///
/// `no_impression` - whether or not an [`IMPRESSION`] event should be sent with `onload`.
///
/// - [`WAIT_FOR_IMPRESSION`] - The time that needs to pass before sending the [`IMPRESSION`] event to all validators.
pub fn get_unit_html_with_events(
    options: &Options,
    ad_unit: &AdUnit,
    hostname: &str,
    campaign_id: CampaignId,
    validators: &Validators,
    no_impression: bool,
) -> String {
    let get_fetch_code = |event_type: EventType| -> String {
        let event = match event_type {
            EventType::Impression => Event::Impression {
                publisher: options.publisher_addr,
                ad_unit: ad_unit.ipfs,
                ad_slot: options.market_slot,
                referrer: Some("document.referrer".to_string()),
            },
            EventType::Click => Event::Click {
                publisher: options.publisher_addr,
                ad_unit: ad_unit.ipfs,
                ad_slot: options.market_slot,
                referrer: Some("document.referrer".to_string()),
            },
        };
        let events_body = InsertEventsRequest {
            events: vec![event],
        };
        let body =
            serde_json::to_string(&events_body).expect("It should always serialize EventBody");

        // TODO: check whether the JSON body with `''` quotes executes correctly!
        let fetch_opts = format!("var fetchOpts = {{ method: 'POST', headers: {{ 'content-type': 'application/json' }}, body: {body} }};");

        let validators: String = validators
            .iter()
            .map(|validator| {
                let fetch_url = format!(
                    "{}/campaign/{}/events?pubAddr={}",
                    validator.url, campaign_id, options.publisher_addr
                );

                format!("fetch('{}', fetchOpts)", fetch_url)
            })
            .collect::<Vec<_>>()
            .join("; ");

        format!("{fetch_opts} {validators}")
    };

    let get_timeout_code = |event_type: EventType| -> String {
        format!(
            "setTimeout(function() {{ {code} }}, {timeout})",
            code = get_fetch_code(event_type),
            timeout = WAIT_FOR_IMPRESSION.num_milliseconds()
        )
    };

    let on_load = if no_impression {
        String::new()
    } else {
        get_timeout_code(IMPRESSION)
    };

    get_unit_html(
        options.size,
        ad_unit,
        hostname,
        &on_load,
        &get_fetch_code(CLICK),
    )
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::manager::DEFAULT_TOKENS;
    use adex_primitives::{
        test_util::{DUMMY_CAMPAIGN, DUMMY_IPFS, PUBLISHER},
        util::ApiUrl,
    };
    use scraper::{Html, Selector};

    fn get_ad_unit(media_mime: &str) -> AdUnit {
        AdUnit {
            ipfs: DUMMY_IPFS[0],
            media_url: "".to_string(),
            media_mime: media_mime.to_string(),
            target_url: "".to_string(),
        }
    }

    #[test]
    fn test_is_video() {
        assert!(is_video(&get_ad_unit("video/avi")));
        assert!(!is_video(&get_ad_unit("image/jpeg")));
    }

    #[test]
    fn normalization_of_url() {
        // IPFS case
        assert_eq!(format!("{}123", IPFS_GATEWAY), normalize_url("ipfs://123"));
        assert_eq!(
            format!("{}123ipfs://", IPFS_GATEWAY),
            normalize_url("ipfs://123ipfs://")
        );

        // Non-IPFS case
        assert_eq!("http://123".to_string(), normalize_url("http://123"));
    }

    #[test]
    fn getting_unit_html() {
        let size_1 = Size {
            width: 480,
            height: 480,
        };

        let size_2 = Size {
            width: 920,
            height: 160,
        };

        let video_unit = AdUnit {
            ipfs: DUMMY_IPFS[0],
            media_url: "".into(),
            media_mime: "video/avi".into(),
            target_url: "https://ambire.com?utm_source=adex_PUBHOSTNAME".into(),
        };

        let image_unit = AdUnit {
            ipfs: DUMMY_IPFS[1],
            media_url: "".into(),
            media_mime: "image/jpeg".into(),
            target_url: "https://ambire.com?utm_source=adex_PUBHOSTNAME".into(),
        };

        // Test for first size, correct link, video inside link
        {
            let unit = get_unit_html(Some(size_1), &video_unit, "https://adex.network", "", "");
            let fragment = Html::parse_fragment(&unit);

            let div_selector = Selector::parse("div").unwrap();
            let div = fragment
                .select(&div_selector)
                .next()
                .expect("There should be a div");

            let anchor_selector = Selector::parse("div>a").unwrap();
            let anchor = fragment
                .select(&anchor_selector)
                .next()
                .expect("There should be an anchor");

            let video_selector = Selector::parse("div>a>video").unwrap();
            let video = fragment
                .select(&video_selector)
                .next()
                .expect("There should be a video");

            assert_eq!("div", div.value().name());

            assert_eq!("a", anchor.value().name());
            assert_eq!(
                Some("https://ambire.com?utm_source=AdEx+(https://adex.network)"),
                anchor.value().attr("href")
            );

            assert_eq!("video", video.value().name());
            assert_eq!(Some("480"), video.value().attr("width"));
            assert_eq!(Some("480"), video.value().attr("height"));
        }
        // Test for another size
        {
            let unit = get_unit_html(Some(size_2), &video_unit, "https://adex.network", "", "");
            let fragment = Html::parse_fragment(&unit);

            let video_selector = Selector::parse("div>a>video").unwrap();
            let video = fragment
                .select(&video_selector)
                .next()
                .expect("There should be a video");

            assert_eq!("video", video.value().name());
            assert_eq!(Some("920"), video.value().attr("width"));
            assert_eq!(Some("160"), video.value().attr("height"));
        }

        // Test for image ad_unit
        {
            let unit = get_unit_html(Some(size_1), &image_unit, "https://adex.network", "", "");
            let fragment = Html::parse_fragment(&unit);

            let image_selector = Selector::parse("div>a>*").unwrap();
            let image = fragment
                .select(&image_selector)
                .next()
                .expect("There should be an image");

            assert_eq!("img", image.value().name());
        }

        // Test for another hostname
        {
            let unit = get_unit_html(
                Some(size_1),
                &video_unit,
                "https://adsense.google.com",
                "",
                "",
            );
            let fragment = Html::parse_fragment(&unit);

            let anchor_selector = Selector::parse("div>a").unwrap();
            let anchor = fragment
                .select(&anchor_selector)
                .next()
                .expect("There should be a second anchor");

            assert_eq!("a", anchor.value().name());
            assert_eq!(
                Some("https://ambire.com?utm_source=AdEx+(https://adsense.google.com)"),
                anchor.value().attr("href")
            );
        }
    }

    #[test]
    fn getting_unit_html_with_events() {
        let whitelisted_tokens = DEFAULT_TOKENS.clone();

        let market_url = ApiUrl::parse("https://market.adex.network").expect("should parse");
        let validator_1_url = ApiUrl::parse("https://tom.adex.network").expect("should parse");
        let validator_2_url = ApiUrl::parse("https://jerry.adex.network").expect("should parse");
        let options = Options {
            market_url,
            market_slot: DUMMY_IPFS[0],
            publisher_addr: *PUBLISHER,
            // All passed tokens must be of the same price and decimals, so that the amounts can be accurately compared
            whitelisted_tokens,
            size: Some(Size::new(300, 100)),
            navigator_language: Some("bg".into()),
            disabled_video: false,
            disabled_sticky: false,
            validators: vec![validator_1_url, validator_2_url],
        };
        let ad_unit = AdUnit {
            ipfs: DUMMY_IPFS[0],
            media_url: "".into(),
            media_mime: "video/avi".into(),
            target_url: "https://ambire.com?utm_source=adex_PUBHOSTNAME".into(),
        };
        let campaign_id = DUMMY_CAMPAIGN.id;
        let validators = DUMMY_CAMPAIGN.validators.clone();
        // Test with events
        {
            let unit_with_events = get_unit_html_with_events(
                &options,
                &ad_unit,
                "https://adex.network",
                campaign_id,
                &validators,
                false,
            );
            let fragment = Html::parse_fragment(&unit_with_events);

            let anchor_selector = Selector::parse("div>a").unwrap();
            let anchor = fragment
                .select(&anchor_selector)
                .next()
                .expect("There should be a second anchor");

            let video_selector = Selector::parse("div>a>video").unwrap();
            let video = fragment
                .select(&video_selector)
                .next()
                .expect("There should be a video");

            // TODO: If campaign.validators doesn't guarantee order this might fail and become untestable
            let expected_onclick: &str = &format!("var fetchOpts = {{ method: 'POST', headers: {{ 'content-type': 'application/json' }}, body: {{'events':[{{'type':'CLICK','publisher':'{}','adUnit':'{}','adSlot':'{}','referrer':'document.referrer'}}]}} }}; fetch('{}/campaign/{}/events?pubAddr={}', fetchOpts); fetch('{}/campaign/{}/events?pubAddr={}', fetchOpts)", options.publisher_addr, ad_unit.ipfs, options.market_slot, validators.iter().nth(0).unwrap().url, campaign_id, options.publisher_addr, validators.iter().nth(1).unwrap().url, campaign_id, options.publisher_addr);
            assert_eq!(Some(expected_onclick), anchor.value().attr("onclick"));

            let expected_onloadeddata: &str = &format!("setTimeout(function() {{ var fetchOpts = {{ method: 'POST', headers: {{ 'content-type': 'application/json' }}, body: {{'events':[{{'type':'IMPRESSION','publisher':'{}','adUnit':'{}','adSlot':'{}','referrer':'document.referrer'}}]}} }}; fetch('{}/campaign/{}/events?pubAddr={}', fetchOpts); fetch('{}/campaign/{}/events?pubAddr={}', fetchOpts) }}, 8000)", options.publisher_addr, ad_unit.ipfs, options.market_slot, validators.iter().nth(0).unwrap().url, campaign_id, options.publisher_addr, validators.iter().nth(1).unwrap().url, campaign_id, options.publisher_addr);
            assert_eq!(
                Some(expected_onloadeddata),
                video.value().attr("onloadeddata")
            );
        }
    }

    mod randomized_sort_pos {

        use super::*;

        #[test]
        fn test_randomized_position() {
            let ad_unit = AdUnit {
                ipfs: DUMMY_IPFS[0],
                media_url: "ipfs://QmWWQSuPMS6aXCbZKpEjPHPUZN2NjB3YrhJTHsV4X3vb2t".to_string(),
                media_mime: "image/jpeg".to_string(),
                target_url: "https://google.com".to_string(),
            };

            let result = randomized_sort_pos(&ad_unit, 5.into());

            // The seed is responsible for generating different results since the AdUnit IPFS can be the same
            assert_eq!(BigNum::from(177_349_401), result);
        }
    }
}
