#![deny(rust_2018_idioms)]
#![deny(clippy::all)]

use adex_primitives::{
    supermarket::units_for_slot,
    supermarket::units_for_slot::response::{AdUnit, Campaign},
    targeting::{self, input},
    BigNum, ChannelId, SpecValidators, ValidatorId, IPFS,
};
use async_std::{sync::RwLock, task::block_on};
use chrono::{DateTime, Utc};
use lazy_static::lazy_static;
use num_integer::Integer;
use rand::Rng;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use slog::{error, Logger};
use std::{
    cmp::Ordering,
    collections::VecDeque,
    convert::TryFrom,
    ops::{Add, Mul},
    sync::Arc,
};
use thiserror::Error;
use units_for_slot::response::UnitsWithPrice;
use url::Url;

const IPFS_GATEWAY: &str = "https://ipfs.moonicorn.network/ipfs/";

// How much time to wait before sending out an impression event
// Related: https://github.com/AdExNetwork/adex-adview-manager/issues/17, https://github.com/AdExNetwork/adex-adview-manager/issues/35, https://github.com/AdExNetwork/adex-adview-manager/issues/46
const WAIT_FOR_IMPRESSION: u32 = 8000;
// The number of impressions (won auctions) kept in history
const HISTORY_LIMIT: u32 = 50;

lazy_static! {
// Impression "stickiness" time: see https://github.com/AdExNetwork/adex-adview-manager/issues/65
// 4 minutes allows ~4 campaigns to rotate, considering a default frequency cap of 15 minutes
    pub static ref IMPRESSION_STICKINESS_TIME: chrono::Duration = chrono::Duration::milliseconds(240000);
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
// TODO: Add Default Ops somehow?
pub struct Options {
    // Defaulted via defaultOpts
    #[serde(rename = "marketURL")]
    pub market_url: Url,
    pub market_slot: IPFS,
    pub publisher_addr: ValidatorId,
    // All passed tokens must be of the same price and decimals, so that the amounts can be accurately compared
    pub whitelisted_tokens: Vec<String>,
    pub width: Option<u64>,
    pub height: Option<u64>,
    pub navigator_language: Option<String>,
    /// Defaulted
    pub disabled_video: bool,
    pub disabled_sticky: bool,
}

impl Options {
    pub fn size(&self) -> Option<(u64, u64)> {
        self.width
            .and_then(|width| self.height.map(|height| (width, height)))
    }
}

#[derive(Debug, Clone)]
pub struct HistoryEntry {
    time: DateTime<Utc>,
    unit_id: IPFS,
    campaign_id: ChannelId,
    slot_id: IPFS,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Event {
    #[serde(rename = "type")]
    event_type: String,
    publisher: ValidatorId,
    ad_unit: IPFS,
    ad_slot: IPFS,
    #[serde(rename = "ref")]
    referrer: String,
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

fn image_html(on_load: &str, size: &Option<(u64, u64)>, image_url: &str) -> String {
    let size = size
        .map(|(width, height)| format!("width=\"{}\" height=\"{}\"", width, height))
        .unwrap_or_else(|| "".to_string());

    format!("<img loading=\"lazy\" src=\"{image_url}\" alt=\"AdEx ad\" rel=\"nofollow\" onload=\"{on_load}\" {size}>",
            image_url = image_url, on_load = on_load, size = size)
}

fn video_html(
    on_load: &str,
    size: &Option<(u64, u64)>,
    image_url: &str,
    media_mime: &str,
) -> String {
    let size = size
        .map(|(width, height)| format!("width=\"{}\" height=\"{}\"", width, height))
        .unwrap_or_else(|| "".to_string());

    format!(
        "<video {size} loop autoplay onloadeddata=\"{on_load}\" muted>
            <source src=\"{image_url}\" type=\"{media_mime}\">
        </video>",
        size = size,
        on_load = on_load,
        image_url = image_url,
        media_mime = media_mime
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
    size: &Option<(u64, u64)>,
    ad_unit: &AdUnit,
    hostname: &str,
    on_load: &str,
    on_click: &str,
) -> String {
    let image_url = normalize_url(&ad_unit.media_url);

    let element_html = if is_video(&ad_unit) {
        video_html(on_load, size, &image_url, &ad_unit.media_mime)
    } else {
        image_html(on_load, size, &image_url)
    };

    // @TODO click protection page
    let final_target_url = ad_unit.target_url.replace(
        "utm_source=adex_PUBHOSTNAME",
        &format!("utm_source=AdEx+({hostname})", hostname = hostname),
    );

    let max_min_size = match size {
        Some((width, height)) => {
            format!(
                "max-width: {}px; min-width: {min_width}px; height: {}px;",
                width,
                height,
                // u64 / 2 will floor the result!
                min_width = width / 2
            )
        }
        None => String::new(),
    };

    format!("<div style=\"position: relative; overflow: hidden; {style}\">
        <a href=\"{final_target_url}\" target=\"_blank\" onclick=\"{on_click}\" rel=\"noopener noreferrer\">
        {element_html}
        </a>
        {adex_icon}
        </div>", style=max_min_size, adex_icon=adex_icon(), final_target_url=final_target_url, on_click = on_click, element_html=element_html)
}

pub fn get_unit_html_with_events(
    options: &Options,
    ad_unit: &AdUnit,
    hostname: &str,
    channel_id: ChannelId,
    validators: &SpecValidators,
    no_impression: impl Into<bool>,
) -> String {
    let get_body = |event_type: &str| EventBody {
        events: vec![Event {
            event_type: event_type.to_string(),
            publisher: options.publisher_addr,
            ad_unit: ad_unit.id.clone(),
            ad_slot: options.market_slot.clone(),
            referrer: "document.referrer".to_string(),
        }],
    };

    let get_fetch_code = |event_type: &str| -> String {
        let body = serde_json::to_string(&get_body(event_type))
            .expect("It should always serialize EventBody");

        let fetch_opts = format!("var fetchOpts = {{ method: 'POST', headers: {{ 'content-type': 'application/json' }}, body: {} }};", body);

        let validators: String = validators
            .iter()
            .map(|validator| {
                let fetch_url = format!(
                    "{}/channel/{}/events?pubAddr={}",
                    validator.url, channel_id, options.publisher_addr
                );

                format!("fetch('{}', fetchOpts)", fetch_url)
            })
            .collect::<Vec<_>>()
            .join(";");

        format!("{}{}", fetch_opts, validators)
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
        &options.size(),
        ad_unit,
        hostname,
        &on_load,
        &get_fetch_code("CLICK"),
    )
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("Request to the Market failed: status {status} at url {url}")]
    Market { status: StatusCode, url: String },
    #[error(transparent)]
    Request(#[from] reqwest::Error),
}

pub struct Manager {
    options: Options,
    /// Contains the Entries from Old to New
    /// It always trims to HISTORY_LIMIT, removing the oldest (firstly inserted) elements from the History
    history: Arc<RwLock<VecDeque<HistoryEntry>>>,
    client: reqwest::Client,
    logger: Logger,
}

impl Manager {
    pub fn new(
        options: Options,
        history: VecDeque<HistoryEntry>,
        logger: Logger,
    ) -> Result<Self, Error> {
        let client = reqwest::Client::builder().build()?;

        Ok(Self {
            options,
            history: Arc::new(RwLock::new(history)),
            client,
            logger,
        })
    }

    pub async fn get_targeting_input(
        &self,
        mut input: input::Input,
        channel_id: ChannelId,
    ) -> input::Input {
        let seconds_since_campaign_impression = self
            .history
            .read()
            .await
            .iter()
            .rev()
            .find_map(|h| {
                if h.campaign_id == channel_id {
                    let last_impression: chrono::Duration = Utc::now() - h.time;

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
            .find(|c| c.channel.id == sticky_entry.campaign_id)?;

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
            stick_campaign.channel.id,
            &stick_campaign.channel.spec.validators,
            true,
        );

        Some(StickyAdUnit {
            unit,
            price: 0.into(),
            html,
            is_sticky: true,
        })
    }

    async fn is_campaign_sticky(&self, campaign_id: ChannelId) -> bool {
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
        let pub_prefix: String = self.options.publisher_addr.to_hex_non_prefix_string();

        let deposit_asset = self
            .options
            .whitelisted_tokens
            .iter()
            .map(|token| format!("depositAsset={}", token))
            .collect::<Vec<_>>()
            .join("&");

        // Url adds a trailing `/`
        let url = format!(
            "{}units-for-slot/{}?pubPrefix={}&{}",
            self.options.market_url, self.options.market_slot, pub_prefix, deposit_asset
        );

        let market_response = self.client.get(&url).send().await?;

        if market_response.status() != StatusCode::OK {
            Err(Error::Market {
                status: market_response.status(),
                url,
            })
        } else {
            let units_for_slot_response = market_response.json().await?;

            Ok(units_for_slot_response)
        }
    }

    pub async fn get_next_ad_unit(&self) -> Result<Option<NextAdUnit>, Error> {
        let units_for_slot = self.get_market_demand_resp().await?;
        let campaigns = &units_for_slot.campaigns;
        let fallback_unit = units_for_slot.fallback_unit;
        let targeting_input = units_for_slot.targeting_input_base;

        let hostname = targeting_input
            .ad_slot
            .as_ref()
            .map(|ad_slot| ad_slot.hostname.clone())
            .unwrap_or_default();

        // Stickiness is when we keep showing an ad unit for a slot for some time in order to achieve fair impression value
        // see https://github.com/AdExNetwork/adex-adview-manager/issues/65
        let sticky_result = self.get_sticky_ad_unit(campaigns, &hostname).await;
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
        let mut units_with_price: Vec<(UnitsWithPrice, ChannelId)> = campaigns
            .iter()
            .map(|campaign| {
                // since we are in a Iterator.map(), we can't use async, so we block
                if block_on(self.is_campaign_sticky(campaign.channel.id)) {
                    return vec![];
                }

                let campaign_id = campaign.channel.id;

                let mut unit_input = targeting_input.clone().with_market_channel(campaign.channel.clone());

                campaign
                    .units_with_price
                    .iter()
                    .filter(|unit_with_price| {
                        unit_input.ad_unit_id = Some(unit_with_price.unit.id.clone());

                        let mut output = targeting::Output {
                            show: true,
                            boost: 1.0,
                            price: vec![("IMPRESSION".to_string(), unit_with_price.price.clone())]
                                .into_iter()
                                .collect(),
                        };

                        let on_type_error = |error, rule| error!(&self.logger, "Rule evaluation error for {:?}", campaign_id; "error" => ?error, "rule" => ?rule);

                        targeting::eval_with_callback(
                            &campaign.targeting_rules,
                            &unit_input,
                            &mut output,
                            Some(on_type_error)
                        );

                        output.show
                    })
                    .map(|uwp| (uwp.clone(), campaign_id))
                    .collect()
            })
            .flatten()
            .filter(|x| !(self.options.disabled_video && is_video(&x.0.unit)))
            .collect();

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
                unit_id: unit_with_price.unit.id.clone(),
                campaign_id: *campaign_id,
                slot_id: self.options.market_slot.clone(),
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
            let validators = campaigns
                .iter()
                .find_map(|campaign| {
                    if &campaign.channel.id == campaign_id {
                        Some(&campaign.channel.spec.validators)
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
                price: unit_with_price.price.clone(),
                accepted_referrers: units_for_slot.accepted_referrers,
                html,
            }))
        } else if let Some(fallback_unit) = fallback_unit {
            let html = get_unit_html(&self.options.size(), &fallback_unit, &hostname, "", "");
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
    pub price: BigNum,
    pub accepted_referrers: Vec<Url>,
    pub html: String,
}

pub struct StickyAdUnit {
    pub unit: AdUnit,
    pub price: BigNum,
    pub html: String,
    pub is_sticky: bool,
}

#[cfg(test)]
mod test {
    use super::*;
    use adex_primitives::util::tests::prep_db::DUMMY_IPFS;

    fn get_ad_unit(media_mime: &str) -> AdUnit {
        AdUnit {
            id: DUMMY_IPFS[0].clone(),
            media_url: "".to_string(),
            media_mime: media_mime.to_string(),
            target_url: "".to_string(),
        }
    }

    #[test]
    fn test_is_video() {
        assert_eq!(true, is_video(&get_ad_unit("video/avi")));
        assert_eq!(false, is_video(&get_ad_unit("image/jpeg")));
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
                id: DUMMY_IPFS[0].clone(),
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
