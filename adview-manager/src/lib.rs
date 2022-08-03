#![deny(rust_2018_idioms)]
#![deny(clippy::all)]
#![deny(rustdoc::broken_intra_doc_links)]
#![cfg_attr(docsrs, feature(doc_cfg))]

use adex_primitives::{
    campaign::Validators,
    sentry::Event,
    supermarket::units_for_slot,
    supermarket::units_for_slot::response::{AdUnit, Campaign},
    targeting::{self, input},
    util::ApiUrl,
    Address, BigNum, CampaignId, ToHex, UnifiedNum, IPFS,
};
use async_std::{sync::RwLock, task::block_on};
use chrono::{DateTime, Duration, Utc};
use log::error;
use num_integer::Integer;
use once_cell::sync::Lazy;
use rand::Rng;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::{
    cmp::Ordering,
    collections::VecDeque,
    ops::{Add, Mul},
    sync::Arc,
};
use thiserror::Error;

pub use url::Url;

const IPFS_GATEWAY: &str = "https://ipfs.moonicorn.network/ipfs/";

/// How much time to wait before sending out an impression event (in milliseconds)
///
/// Related: <https://github.com/AdExNetwork/adex-adview-manager/issues/17>, <https://github.com/AdExNetwork/adex-adview-manager/issues/35>, <https://github.com/AdExNetwork/adex-adview-manager/issues/46>
///
/// Used for JS [setTimeout](https://developer.mozilla.org/en-US/docs/Web/API/setTimeout) function
pub const WAIT_FOR_IMPRESSION: u32 = 8000;
// The number of impressions (won auctions) kept in history
const HISTORY_LIMIT: u32 = 50;

/// Impression "stickiness" time: see <https://github.com/AdExNetwork/adex-adview-manager/issues/65>
/// 4 minutes allows ~4 campaigns to rotate, considering a default frequency cap of 15 minutes
pub static IMPRESSION_STICKINESS_TIME: Lazy<Duration> =
    Lazy::new(|| Duration::milliseconds(240_000));

// AdSlot size `width x height` in `pixels` (`px`)
#[derive(Debug, Serialize, Deserialize, Clone, Copy, PartialEq, Eq)]
pub struct Size {
    pub width: u64,
    pub height: u64,
}

impl Size {
    pub fn new(width: u64, height: u64) -> Self {
        Self { width, height }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
// TODO: Add Default Ops somehow?
pub struct Options {
    // Defaulted via defaultOpts
    #[serde(rename = "marketURL")]
    pub market_url: ApiUrl,
    pub market_slot: IPFS,
    pub publisher_addr: Address,
    // All passed tokens must be of the same price and decimals, so that the amounts can be accurately compared
    pub whitelisted_tokens: Vec<Address>,
    pub size: Option<Size>,
    pub navigator_language: Option<String>,
    /// Defaulted
    #[serde(default)]
    pub disabled_video: bool,
    #[serde(default)]
    pub disabled_sticky: bool,
}

#[derive(Debug, Clone)]
pub struct HistoryEntry {
    time: DateTime<Utc>,
    unit_id: IPFS,
    campaign_id: CampaignId,
    slot_id: IPFS,
}

#[derive(Serialize)]
struct EventBody {
    events: Vec<Event>,
}

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

fn is_video(ad_unit: &AdUnit) -> bool {
    ad_unit.media_mime.split('/').next() == Some("video")
}

/// Does not copy the JS impl, instead it generates the BigNum from the IPFS CID bytes
fn randomized_sort_pos(ad_unit: &AdUnit, seed: BigNum) -> BigNum {
    let bytes = ad_unit.id.0.to_bytes();

    let unit_id = BigNum::from_bytes_be(&bytes);

    let x: BigNum = unit_id.mul(seed).add(BigNum::from(12345));

    x.mod_floor(&BigNum::from(0x80000000))
}

fn get_unit_html(
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

/// no_impression - whether or not an IMPRESSION event should be sent with `onload`
///
///   [`WAIT_FOR_IMPRESSION`] - The timeout used before sending the IMPRESSION event to all validators
pub fn get_unit_html_with_events(
    options: &Options,
    ad_unit: &AdUnit,
    hostname: &str,
    campaign_id: CampaignId,
    validators: &Validators,
    no_impression: impl Into<bool>,
) -> String {
    let get_fetch_code = |event_type: &str| -> String {
        let event = match event_type {
            "CLICK" => Event::Click {
                publisher: options.publisher_addr,
                ad_unit: ad_unit.id,
                ad_slot: options.market_slot,
                referrer: Some("document.referrer".to_string()),
            },
            _ => Event::Impression {
                publisher: options.publisher_addr,
                ad_unit: ad_unit.id,
                ad_slot: options.market_slot,
                referrer: Some("document.referrer".to_string()),
            },
        };
        let event_body = EventBody {
            events: vec![event],
        };
        let body =
            serde_json::to_string(&event_body).expect("It should always serialize EventBody");

        // TODO: check whether the JSON body with `''` quotes executes correctly!
        let fetch_opts = format!("var fetchOpts = {{ method: 'POST', headers: {{ 'content-type': 'application/json' }}, body: {} }};", body);

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

    let get_timeout_code = |event_type: &str| -> String {
        format!(
            "setTimeout(function() {{ {code} }}, {timeout})",
            code = get_fetch_code(event_type),
            timeout = WAIT_FOR_IMPRESSION
        )
    };

    let on_load = if no_impression.into() {
        String::new()
    } else {
        get_timeout_code("IMPRESSION")
    };

    get_unit_html(
        options.size,
        ad_unit,
        hostname,
        &on_load,
        &get_fetch_code("CLICK"),
    )
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("Request to the Market failed: status {status} at url {url}")]
    Market { status: StatusCode, url: Url },
    #[error(transparent)]
    Request(#[from] reqwest::Error),
}

pub struct Manager {
    options: Options,
    /// Contains the Entries from Old to New
    /// It always trims to HISTORY_LIMIT, removing the oldest (firstly inserted) elements from the History
    history: Arc<RwLock<VecDeque<HistoryEntry>>>,
    client: reqwest::Client,
}

impl Manager {
    pub fn new(options: Options, history: VecDeque<HistoryEntry>) -> Result<Self, Error> {
        let client = reqwest::Client::builder().build()?;

        Ok(Self {
            options,
            history: Arc::new(RwLock::new(history)),
            client,
        })
    }

    pub async fn get_targeting_input(
        &self,
        mut input: input::Input,
        campaign_id: CampaignId,
    ) -> input::Input {
        let seconds_since_campaign_impression = self
            .history
            .read()
            .await
            .iter()
            .rev()
            .find_map(|h| {
                if h.campaign_id == campaign_id {
                    let last_impression: Duration = Utc::now() - h.time;

                    u64::try_from(last_impression.num_seconds()).ok()
                } else {
                    None
                }
            })
            .unwrap_or(u64::MAX);

        input.ad_view = Some(input::AdView {
            seconds_since_campaign_impression,
            has_custom_preferences: false,
            navigator_language: self.options.navigator_language.clone().unwrap_or_default(),
        });

        input
    }

    pub async fn get_sticky_ad_unit(
        &self,
        campaigns: &[Campaign],
        hostname: &str,
    ) -> Option<StickyAdUnit> {
        if self.options.disabled_sticky {
            return None;
        }

        let stickiness_threshold = Utc::now() - *IMPRESSION_STICKINESS_TIME;
        let sticky_entry = self
            .history
            .read()
            .await
            .iter()
            .find(|h| h.time > stickiness_threshold && h.slot_id == self.options.market_slot)
            .cloned()?;

        let stick_campaign = campaigns
            .iter()
            .find(|c| c.campaign.id == sticky_entry.campaign_id)?;

        let unit = stick_campaign
            .units_with_price
            .iter()
            .find_map(|u| {
                if u.unit.id == sticky_entry.unit_id {
                    Some(u.unit.clone())
                } else {
                    None
                }
            })
            .expect("Something went terribly wrong. Data is corrupted! There should be an AdUnit");

        let html = get_unit_html_with_events(
            &self.options,
            &unit,
            hostname,
            stick_campaign.campaign.id,
            &stick_campaign.campaign.validators,
            true,
        );

        Some(StickyAdUnit {
            unit,
            price: 0.into(),
            html,
            is_sticky: true,
        })
    }

    async fn is_campaign_sticky(&self, campaign_id: CampaignId) -> bool {
        if self.options.disabled_sticky {
            false
        } else {
            let stickiness_threshold = Utc::now() - *IMPRESSION_STICKINESS_TIME;

            self.history
                .read()
                .await
                .iter()
                .any(|h| h.time > stickiness_threshold && h.campaign_id == campaign_id)
        }
    }

    pub async fn get_market_demand_resp(
        &self,
    ) -> Result<units_for_slot::response::Response, Error> {
        let pub_prefix = self.options.publisher_addr.to_hex();

        let deposit_asset = self
            .options
            .whitelisted_tokens
            .iter()
            .map(|token| format!("depositAsset={}", token))
            .collect::<Vec<_>>()
            .join("&");

        // ApiUrl handles endpoint path (with or without `/`)
        let url = self
            .options
            .market_url
            .join(&format!(
                "units-for-slot/{ad_slot}?pubPrefix={pub_prefix}&{deposit_asset}",
                ad_slot = self.options.market_slot
            ))
            .expect("Valid URL endpoint!");

        let market_response = self.client.get(url.clone()).send().await?;

        match market_response.status() {
            StatusCode::OK => Ok(market_response.json().await?),
            _ => Err(Error::Market {
                status: market_response.status(),
                url,
            }),
        }
    }

    pub async fn get_next_ad_unit(&self) -> Result<Option<NextAdUnit>, Error> {
        let units_for_slot = self.get_market_demand_resp().await?;
        let m_campaigns = &units_for_slot.campaigns;
        let fallback_unit = units_for_slot.fallback_unit;
        let targeting_input = units_for_slot.targeting_input_base;

        let hostname = targeting_input
            .ad_slot
            .as_ref()
            .map(|ad_slot| ad_slot.hostname.clone())
            .unwrap_or_default();

        // Stickiness is when we keep showing an ad unit for a slot for some time in order to achieve fair impression value
        // see https://github.com/AdExNetwork/adex-adview-manager/issues/65
        let sticky_result = self.get_sticky_ad_unit(m_campaigns, &hostname).await;
        if let Some(sticky) = sticky_result {
            return Ok(Some(NextAdUnit {
                unit: sticky.unit,
                price: sticky.price,
                accepted_referrers: units_for_slot.accepted_referrers,
                html: sticky.html,
            }));
        }

        // If two or more units result in the same price, apply random selection between them: this is why we need the seed
        let mut rng = rand::thread_rng();

        let random: f64 = rng.gen::<f64>() * (0x80000000_u64 as f64 - 1.0);
        let seed = BigNum::from(random as u64);

        // Apply targeting, now with adView.* variables, and sort the resulting ad units
        let mut units_with_price = m_campaigns
            .iter()
            .flat_map(|m_campaign| {
                // since we are in a Iterator.map(), we can't use async, so we block
                if block_on(self.is_campaign_sticky(m_campaign.campaign.id)) {
                    return vec![];
                }

                let campaign_id = m_campaign.campaign.id;

                let mut unit_input = targeting_input.clone().with_campaign(m_campaign.campaign.clone());

                m_campaign
                    .units_with_price
                    .iter()
                    .filter(|unit_with_price| {
                        unit_input.ad_unit_id = Some(unit_with_price.unit.id);

                        let mut output = targeting::Output {
                            show: true,
                            boost: 1.0,
                            price: vec![("IMPRESSION".to_string(), unit_with_price.price)]
                                .into_iter()
                                .collect(),
                        };

                        let on_type_error = |type_error, rule| error!(target: "rule-evaluation", "Rule evaluation error for {campaign_id:?}, {rule:?} with error: {type_error:?}");

                        targeting::eval_with_callback(
                            &m_campaign.campaign.targeting_rules,
                            &unit_input,
                            &mut output,
                            Some(on_type_error)
                        );

                        output.show
                    })
                    .map(|uwp| (uwp.clone(), campaign_id))
                    .collect()
            })
            .filter(|x| !(self.options.disabled_video && is_video(&x.0.unit)))
            .collect::<Vec<_>>();

        units_with_price.sort_by(|b, a| match (&a.0.price).cmp(&b.0.price) {
            Ordering::Equal => randomized_sort_pos(&a.0.unit, seed.clone())
                .cmp(&randomized_sort_pos(&b.0.unit, seed.clone())),
            ordering => ordering,
        });

        // Update history
        let auction_winner = units_with_price.get(0);

        if let Some((unit_with_price, campaign_id)) = auction_winner {
            let history = self.history.read().await.clone();

            let new_entry = HistoryEntry {
                time: Utc::now(),
                unit_id: unit_with_price.unit.id,
                campaign_id: *campaign_id,
                slot_id: self.options.market_slot,
            };

            *self.history.write().await = history
                .into_iter()
                .chain(std::iter::once(new_entry))
                // Reverse the iterator since we want to remove the oldest history entries
                .rev()
                .take(HISTORY_LIMIT as usize)
                .collect::<VecDeque<HistoryEntry>>()
                // Keeps the same order, as the one we've started with! Old => New
                .into_iter()
                .rev()
                .collect();
        }

        // Return the results, with a fallback unit if there is one
        if let Some((unit_with_price, campaign_id)) = auction_winner {
            let validators = m_campaigns
                .iter()
                .find_map(|m_campaign| {
                    if &m_campaign.campaign.id == campaign_id {
                        Some(&m_campaign.campaign.validators)
                    } else {
                        None
                    }
                })
                // TODO: Check what should happen here if we don't find the Validator
                .unwrap();

            let html = get_unit_html_with_events(
                &self.options,
                &unit_with_price.unit,
                &hostname,
                *campaign_id,
                validators,
                false,
            );

            Ok(Some(NextAdUnit {
                unit: unit_with_price.unit.clone(),
                price: unit_with_price.price,
                accepted_referrers: units_for_slot.accepted_referrers,
                html,
            }))
        } else if let Some(fallback_unit) = fallback_unit {
            let html = get_unit_html(self.options.size, &fallback_unit, &hostname, "", "");
            Ok(Some(NextAdUnit {
                unit: fallback_unit,
                price: 0.into(),
                accepted_referrers: units_for_slot.accepted_referrers,
                html,
            }))
        } else {
            Ok(None)
        }
    }
}

pub struct NextAdUnit {
    pub unit: AdUnit,
    pub price: UnifiedNum,
    pub accepted_referrers: Vec<Url>,
    pub html: String,
}

pub struct StickyAdUnit {
    pub unit: AdUnit,
    pub price: UnifiedNum,
    pub html: String,
    pub is_sticky: bool,
}

#[cfg(test)]
mod test {
    use super::*;
    use adex_primitives::test_util::DUMMY_IPFS;

    fn get_ad_unit(media_mime: &str) -> AdUnit {
        AdUnit {
            id: DUMMY_IPFS[0],
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

    mod randomized_sort_pos {

        use super::*;

        #[test]
        fn test_randomized_position() {
            let ad_unit = AdUnit {
                id: DUMMY_IPFS[0],
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
