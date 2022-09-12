//! The AdView Manager
use adex_primitives::{
    sentry::{
        units_for_slot::response::{AdUnit, Campaign, Response},
        IMPRESSION,
    },
    targeting::{self, input},
    util::ApiUrl,
    Address, BigNum, CampaignId, ToHex, UnifiedNum, IPFS,
};
use async_std::{sync::RwLock, task::block_on};
use chrono::{DateTime, Duration, Utc};
use log::error;
use once_cell::sync::Lazy;
use rand::Rng;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use std::{
    cmp::Ordering,
    collections::{HashSet, VecDeque},
    sync::Arc,
};
use thiserror::Error;

use crate::{
    get_unit_html_with_events,
    helpers::{get_unit_html, is_video, randomized_sort_pos},
    Url, IMPRESSION_STICKINESS_TIME,
};

/// The number of impressions (won auctions) kept in history
const HISTORY_LIMIT: u32 = 50;

/// Default Market Url that can be used for the [`Options`].
pub static DEFAULT_MARKET_URL: Lazy<ApiUrl> =
    Lazy::new(|| "https://market.moonicorn.network".parse().unwrap());

/// Default Whitelisted token [`Address`]es that can be used for the [`Options`].
pub static DEFAULT_TOKENS: Lazy<HashSet<Address>> = Lazy::new(|| {
    [
        // DAI
        "0x6B175474E89094C44Da98b954EedeAC495271d0F"
            .parse()
            .unwrap(),
    ]
    .into_iter()
    .collect()
});

#[derive(Debug, Error)]
pub enum Error {
    #[error("Request to the Market failed: status {status} at url {url}")]
    Market { status: StatusCode, url: Url },
    #[error(transparent)]
    Request(#[from] reqwest::Error),
}

/// The Ad [`Manager`]'s options for showing ads.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Options {
    #[serde(rename = "marketURL")]
    pub market_url: ApiUrl,
    pub market_slot: IPFS,
    pub publisher_addr: Address,
    /// All passed tokens must be of the same price and decimals, so that the amounts can be accurately compared
    pub whitelisted_tokens: HashSet<Address>,
    pub size: Option<Size>,
    pub navigator_language: Option<String>,
    /// Whether or not to disable Video ads.
    ///
    /// default: `false`
    #[serde(default)]
    pub disabled_video: bool,
    /// Whether or not to disable Sticky Ads ([`AdUnit`]s).
    ///
    /// default: `false`
    #[serde(default)]
    pub disabled_sticky: bool,
}

/// [`AdSlot`](adex_primitives::AdSlot) size `width x height` in pixels (`px`)
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

/// The next [`AdUnit`] to be shown
#[derive(Debug, Clone)]
pub struct NextAdUnit {
    pub unit: AdUnit,
    pub price: UnifiedNum,
    pub accepted_referrers: Vec<Url>,
    pub html: String,
}

/// A sticky [`AdUnit`].
#[derive(Debug, Clone)]
pub struct StickyAdUnit {
    pub unit: AdUnit,
    pub price: UnifiedNum,
    pub html: String,
    pub is_sticky: bool,
}

/// History entry of impressions (won auctions) which the [`Manager`] holds.
#[derive(Debug, Clone)]
pub struct HistoryEntry {
    pub time: DateTime<Utc>,
    pub unit_id: IPFS,
    pub campaign_id: CampaignId,
    pub slot_id: IPFS,
}

/// The AdView Manager
#[derive(Debug, Clone)]
pub struct Manager {
    options: Options,
    /// Contains the Entries from Old to New
    /// It always trims to [`HISTORY_LIMIT`], removing the oldest (firstly inserted) elements from the History
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

    /// Get a sticky [`AdUnit`] if they are not disabled (see [`Options.disabled_sticky`](Options::disabled_sticky)).
    ///
    /// Takes into account the History Entries, [`IMPRESSION_STICKINESS_TIME`]
    /// and the provided [`AdSlot`](adex_primitives::AdSlot) [`IPFS`] from [`Options`].
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
                if u.unit.ipfs == sticky_entry.unit_id {
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

    pub async fn get_market_demand_resp(&self) -> Result<Response, Error> {
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
                        unit_input.ad_unit_id = Some(unit_with_price.unit.ipfs);

                        let mut output = targeting::Output {
                            show: true,
                            boost: 1.0,
                            price: [(IMPRESSION, unit_with_price.price)].into_iter().collect(),
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

        units_with_price.sort_by(|b, a| match a.0.price.cmp(&b.0.price) {
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
                unit_id: unit_with_price.unit.ipfs,
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
