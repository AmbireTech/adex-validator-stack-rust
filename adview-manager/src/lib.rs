#![deny(rust_2018_idioms)]
#![deny(clippy::all)]

use adex_primitives::{
    supermarket::units_for_slot::response::{AdUnit, Campaign, Response},
    targeting::{AdView, Input},
    BigNum, ChannelId, SpecValidators,
};
use chrono::{DateTime, Utc};
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use std::convert::TryFrom;

pub type TargetingScore = f64;
pub type MinTargetingScore = TargetingScore;

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
// const defaultOpts = {
// 	marketURL: 'https://market.moonicorn.network',
// 	whitelistedTokens: ['0x6B175474E89094C44Da98b954EedeAC495271d0F'],
// 	disableVideo: false,
// }
pub struct Options {
    // Defaulted via defaultOpts
    #[serde(rename = "marketURL")]
    pub market_url: String,
    pub market_slot: String,
    pub publisher_addr: String,
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

pub struct HistoryEntry {
    time: DateTime<Utc>,
    unit_id: String,
    channel_id: ChannelId,
    slot_id: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Event {
    #[serde(rename = "type")]
    event_type: String,
    publisher: String,
    ad_unit: String,
    ad_slot: String,
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

// @TODO: IMPL
// function randomizedSortPos(unit: Unit, seed: BN): BN {
// 	// using base32 is technically wrong (IDs are in base 58), but it works well enough for this purpose
// 	// kind of a LCG PRNG but without the state; using GCC's constraints as seen on stack overflow
// 	// takes around ~700ms for 100k iterations, yields very decent distribution (e.g. 724ms 50070, 728ms 49936)
// 	return new BN(unit.id, 32).mul(seed).add(new BN(12345)).mod(new BN(0x80000000))
// }
fn randomized_sort_pos(_ad_unit: AdUnit, _seed: BigNum) -> BigNum {
    todo!("Implement the randomized_sort_pos() function!")
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
            publisher: options.publisher_addr.clone(),
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

pub struct Manager {
    options: Options,
    history: Vec<HistoryEntry>,
}

impl Manager {
    pub fn new(options: Options, history: Vec<HistoryEntry>) -> Self {
        Self { options, history }
    }

    pub fn get_targeting_input(&self, mut input: Input, channel_id: ChannelId) -> Input {
        let seconds_since_campaign_impression = self
            .history
            .iter()
            .rev()
            .find_map(|h| {
                if h.channel_id == channel_id {
                    let last_impression: chrono::Duration = Utc::now() - h.time;

                    u64::try_from(last_impression.num_seconds()).ok()
                } else {
                    None
                }
            })
            .unwrap_or(u64::MAX);

        input.ad_view = Some(AdView {
            seconds_since_campaign_impression,
            has_custom_preferences: false,
            // TODO: Check this empty default!
            navigator_language: self.options.navigator_language.clone().unwrap_or_default(),
        });

        input
    }

    pub fn get_sticky_ad_unit(
        &self,
        campaigns: Vec<Campaign>,
        hostname: &str,
    ) -> Option<StickyAdUnit> {
        if self.options.disabled_sticky {
            return None;
        }

        let stickiness_threshold = Utc::now() - *IMPRESSION_STICKINESS_TIME;
        let sticky_entry = self
            .history
            .iter()
            .find(|h| h.time > stickiness_threshold && h.slot_id == self.options.market_slot)?;

        let stick_campaign = campaigns
            .iter()
            .find(|c| c.channel.id == sticky_entry.channel_id)?;

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

    // private isCampaignSticky(campaign: any): boolean {
    // 	if (this.options.disableSticky) return false
    // 	const stickinessThreshold = Date.now() - IMPRESSION_STICKINESS_TIME
    // 	return !!this.history.find(entry => entry.time > stickinessThreshold && entry.campaignId === campaign.id)
    // }
    fn is_campaign_sticky(channel_id: ChannelId) -> bool {
        // let stickiness_threshold = Utc::now() - *IMPRESSION_STICKINESS_TIME;
        todo!()
    }

    // async getMarketDemandResp(): Promise<any> {
    // 	const marketURL = this.options.marketURL
    // 	const depositAsset = this.options.whitelistedTokens.map(tokenAddr => `&depositAsset=${tokenAddr}`).join('')
    // 	// @NOTE not the same as the WAF generator script in the Market (which uses `.slice(2, 12)`)
    // 	const pubPrefix = this.options.publisherAddr.slice(2, 10)
    // 	const url = `${marketURL}/units-for-slot/${this.options.marketSlot}?pubPrefix=${pubPrefix}${depositAsset}`
    // 	const r = await this.fetch(url)
    // 	if (r.status !== 200) throw new Error(`market returned status code ${r.status} at ${url}`)
    // 	return r.json()
    // }
    pub async fn get_market_demand_resp() {
        todo!()
    }

    // async getNextAdUnit(): Promise<any> {
    // 	const { campaigns, targetingInputBase, acceptedReferrers, fallbackUnit } = await this.getMarketDemandResp()
    // 	const hostname = targetingInputBase['adSlot.hostname']

    // 	// Stickiness is when we keep showing an ad unit for a slot for some time in order to achieve fair impression value
    // 	// see https://github.com/AdExNetwork/adex-adview-manager/issues/65
    // 	const stickyResult = this.getStickyAdUnit(campaigns, hostname)
    // 	if (stickyResult) return { ...stickyResult, acceptedReferrers }

    // 	// If two or more units result in the same price, apply random selection between them: this is why we need the seed
    // 	const seed = new BN(Math.random() * (0x80000000 - 1))

    // 	// Apply targeting, now with adView.* variables, and sort the resulting ad units
    // 	const unitsWithPrice = campaigns
    // 		.map(campaign => {
    // 			if (this.isCampaignSticky(campaign)) return []

    // 			const campaignInputBase = this.getTargetingInput(targetingInputBase, campaign)
    // 			const campaignInput = targetingInputGetter.bind(null, campaignInputBase, campaign)
    // 			const onTypeErr = (e, rule) => console.error(`WARNING: rule for ${campaign.id} failing with:`, rule, e)
    // 			return campaign.unitsWithPrice.filter(({ unit, price }) => {
    // 				const input = campaignInput.bind(null, unit)
    // 				const output = {
    // 					show: true,
    // 					'price.IMPRESSION': new BN(price),
    // 				}
    // 				// NOTE: not using the price from the output on purpose
    // 				// we trust what the server gives us since otherwise we may end up changing the price based on
    // 				// adView-specific variables, which won't be consistent with the validator's price
    // 				return evaluateMultiple(input, output, campaign.targetingRules, onTypeErr).show
    // 			}).map(x => ({ ...x, campaignId: campaign.id }))
    // 		})
    // 		.reduce((a, b) => a.concat(b), [])
    // 		.filter(x => !(this.options.disableVideo && isVideo(x.unit)))
    // 		.sort((b, a) =>
    // 			new BN(a.price).cmp(new BN(b.price))
    // 				|| randomizedSortPos(a.unit, seed).cmp(randomizedSortPos(b.unit, seed))
    // 		)

    // 	// Update history
    // 	const auctionWinner = unitsWithPrice[0]
    // 	if (auctionWinner) {
    // 		this.history.push({
    // 			time: Date.now(),
    // 			slotId: this.options.marketSlot,
    // 			unitId: auctionWinner.unit.id,
    // 			campaignId: auctionWinner.campaignId,
    // 		})
    // 		this.history = this.history.slice(-HISTORY_LIMIT)
    // 	}

    // 	// Return the results, with a fallback unit if there is one
    // 	if (auctionWinner) {
    // 		const { unit, price, campaignId } = auctionWinner
    // 		const { validators } = campaigns.find(x => x.id === campaignId).spec
    // 		return {
    // 			unit,
    // 			price,
    // 			acceptedReferrers,
    // 			html: getUnitHTMLWithEvents(this.options, { unit, hostname, campaignId, validators })
    // 		}
    // 	} else if (fallbackUnit) {
    // 		const unit = fallbackUnit
    // 		return {
    // 			unit,
    // 			price: '0',
    // 			acceptedReferrers,
    // 			html: getUnitHTML(this.options, { unit, hostname })
    // 		}
    // 	} else {
    // 		return null
    // 	}
    // }
    pub async fn get_next_ad_unit() -> Option<Response> {
        todo!()
    }
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

    fn get_ad_unit(media_mime: &str) -> AdUnit {
        AdUnit {
            id: "".to_string(),
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
}
