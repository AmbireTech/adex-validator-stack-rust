//! The AdView Manager
use adex_primitives::{
    sentry::{
        units_for_slot::response::{AdUnit, Campaign, Response},
        IMPRESSION,
    },
    targeting::{self, input},
    util::ApiUrl,
    Address, BigNum, CampaignId, UnifiedNum, IPFS,
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
    #[error("Request to the Sentry failed: status {status} at url {url}")]
    Sentry { status: StatusCode, url: Url },
    #[error(transparent)]
    Request(#[from] reqwest::Error),
    #[error("No validators provided")]
    NoValidators,
    #[error("Invalid validator URL")]
    InvalidValidatorUrl,
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
    /// List of validators to query /units-for-slot from
    pub validators: Vec<ApiUrl>,
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
    // Price per 1 event
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
        let seconds_since_campaign_impression =
            self.history.read().await.iter().rev().find_map(|h| {
                if h.campaign_id == campaign_id {
                    let last_impression: Duration = Utc::now() - h.time;

                    u64::try_from(last_impression.num_seconds()).ok()
                } else {
                    None
                }
            });

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

        let unit = stick_campaign.units_with_price.iter().find_map(|u| {
            if u.unit.ipfs == sticky_entry.unit_id {
                Some(u.unit.clone())
            } else {
                None
            }
        })?;

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

    // Test with different units with price
    // Test if first campaign is not overwritten
    pub async fn get_units_for_slot_resp(&self) -> Result<Response, Error> {
        let deposit_assets = self
            .options
            .whitelisted_tokens
            .iter()
            .map(|token| format!("depositAssets[]={}", token))
            .collect::<Vec<_>>()
            .join("&");

        let first_validator = self.options.validators.get(0).ok_or(Error::NoValidators)?;

        let url = first_validator
            .join(&format!(
                "v5/units-for-slot/{}?{}",
                self.options.market_slot, deposit_assets
            ))
            .map_err(|_| Error::InvalidValidatorUrl)?;
        // Ordering of the campaigns matters so we will just push them to the first result
        // We reuse `targeting_input_base`, `accepted_referrers` and `fallback_unit`
        let mut first_res: Response = self.client.get(url.as_str()).send().await?.json().await?;

        for validator in self.options.validators.iter().skip(1) {
            let url = validator
                .join(&format!(
                    "v5/units-for-slot/{}?{}",
                    self.options.market_slot, deposit_assets
                ))
                .map_err(|_| Error::InvalidValidatorUrl)?;
            let new_res: Response = self.client.get(url.as_str()).send().await?.json().await?;
            for response_campaign in new_res.campaigns {
                if !first_res
                    .campaigns
                    .iter()
                    .any(|c| c.campaign.id == response_campaign.campaign.id)
                {
                    first_res.campaigns.push(response_campaign);
                }
            }
        }

        Ok(first_res)
    }

    pub async fn get_next_ad_unit(&self) -> Result<Option<NextAdUnit>, Error> {
        let units_for_slot = self.get_units_for_slot_resp().await?;
        let ufs_campaigns = &units_for_slot.campaigns;
        let fallback_unit = units_for_slot.fallback_unit;
        let targeting_input = units_for_slot.targeting_input_base;

        let hostname = targeting_input
            .ad_slot
            .as_ref()
            .map(|ad_slot| ad_slot.hostname.clone())
            .unwrap_or_default();

        // Stickiness is when we keep showing an ad unit for a slot for some time in order to achieve fair impression value
        // see https://github.com/AdExNetwork/adex-adview-manager/issues/65
        let sticky_result = self.get_sticky_ad_unit(ufs_campaigns, &hostname).await;
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
        let mut units_with_price = ufs_campaigns
            .iter()
            .flat_map(|ufs_campaign| {
                // since we are in a Iterator.map(), we can't use async, so we block
                if block_on(self.is_campaign_sticky(ufs_campaign.campaign.id)) {
                    return vec![];
                }

                let campaign_id = ufs_campaign.campaign.id;

                let mut unit_input = targeting_input.clone().with_campaign(ufs_campaign.campaign.clone());

                ufs_campaign
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
                            &ufs_campaign.campaign.targeting_rules,
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
            let validators = ufs_campaigns
                .iter()
                .find_map(|ufs_campaign| {
                    if &ufs_campaign.campaign.id == campaign_id {
                        Some(&ufs_campaign.campaign.validators)
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

#[cfg(test)]
mod test {
    use super::*;
    use crate::manager::input::Input;
    use adex_primitives::{
        config::GANACHE_CONFIG,
        sentry::{
            units_for_slot::response::{AdUnit, UnitsWithPrice},
            CLICK,
        },
        test_util::{CAMPAIGNS, DUMMY_AD_UNITS, DUMMY_CAMPAIGN, DUMMY_IPFS, PUBLISHER},
        unified_num::FromWhole,
    };
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };

    fn setup_manager(uri: String) -> Manager {
        let market_url = uri.parse().unwrap();
        let whitelisted_tokens = GANACHE_CONFIG
            .chains
            .values()
            .flat_map(|chain| chain.tokens.values().map(|token| token.address))
            .collect::<HashSet<_>>();

        let validator_1_url = ApiUrl::parse(&format!("{}/validator-1", uri)).expect("should parse");
        let validator_2_url = ApiUrl::parse(&format!("{}/validator-2", uri)).expect("should parse");
        let validator_3_url = ApiUrl::parse(&format!("{}/validator-3", uri)).expect("should parse");

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
            validators: vec![validator_1_url, validator_2_url, validator_3_url],
        };

        Manager::new(options.clone(), Default::default()).expect("Failed to create AdView Manager")
    }
    #[tokio::test]
    async fn test_querying_for_units_for_slot() {
        // 1. Set up mock servers for each validator
        let server = MockServer::start().await;
        let slot = DUMMY_IPFS[0];
        let seconds_since_epoch = Utc::now();

        let original_input = Input {
            ad_view: None,
            global: input::Global {
                ad_slot_id: DUMMY_IPFS[0],
                ad_slot_type: "legacy_250x250".to_string(),
                publisher_id: *PUBLISHER,
                country: None,
                event_type: IMPRESSION,
                // we can't know only the timestamp
                seconds_since_epoch,
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

        let modified_input = Input {
            ad_view: None,
            global: input::Global {
                ad_slot_id: DUMMY_IPFS[1],
                ad_slot_type: "legacy_250x250".to_string(),
                publisher_id: *PUBLISHER,
                country: None,
                event_type: CLICK,
                // we can't know only the timestamp
                seconds_since_epoch,
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

        let original_referrers = vec![Url::parse("https://ambire.com").expect("should parse")];
        let modified_referrers =
            vec![Url::parse("https://www.google.com/adsense/start/").expect("should parse")];

        let original_ad_unit = AdUnit::from(&DUMMY_AD_UNITS[0]);
        let modified_ad_unit = AdUnit::from(&DUMMY_AD_UNITS[1]);

        let campaign_0 = Campaign {
            campaign: CAMPAIGNS[0].context.clone(),
            units_with_price: Vec::new(),
        };

        let campaign_1 = Campaign {
            campaign: CAMPAIGNS[1].context.clone(),
            units_with_price: Vec::new(),
        };

        let campaign_2 = Campaign {
            campaign: CAMPAIGNS[2].context.clone(),
            units_with_price: Vec::new(),
        };

        // Original response
        let response_1 = Response {
            targeting_input_base: original_input.clone(),
            accepted_referrers: original_referrers.clone(),
            fallback_unit: Some(original_ad_unit.clone()),
            campaigns: vec![campaign_0.clone()],
        };

        // Different targeting_input_base, fallback_unit, accepted_referrers, 1 new campaign and 1 repeating campaign
        let response_2 = Response {
            targeting_input_base: modified_input.clone(),
            accepted_referrers: modified_referrers.clone(),
            fallback_unit: Some(modified_ad_unit.clone()),
            campaigns: vec![campaign_0.clone(), campaign_1.clone()],
        };

        // 1 new campaigns, 2 repeating campaigns
        let response_3 = Response {
            targeting_input_base: modified_input,
            accepted_referrers: modified_referrers,
            fallback_unit: Some(modified_ad_unit),
            campaigns: vec![campaign_0.clone(), campaign_1.clone(), campaign_2.clone()],
        };

        Mock::given(method("GET"))
            .and(path(format!("validator-1/v5/units-for-slot/{}", slot)))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_1))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path(format!("validator-2/v5/units-for-slot/{}", slot)))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_2))
            .mount(&server)
            .await;

        Mock::given(method("GET"))
            .and(path(format!("validator-3/v5/units-for-slot/{}", slot,)))
            .respond_with(ResponseTemplate::new(200).set_body_json(&response_3))
            .mount(&server)
            .await;

        // 2. Set up a manager
        let manager = setup_manager(server.uri());

        let res = manager
            .get_units_for_slot_resp()
            .await
            .expect("Should get response");
        assert_eq!(res.targeting_input_base.global.ad_slot_id, DUMMY_IPFS[0]);
        assert_eq!(res.accepted_referrers, original_referrers);
        assert_eq!(res.fallback_unit, Some(original_ad_unit));
        assert_eq!(res.campaigns, vec![campaign_0, campaign_1, campaign_2]);
    }

    #[tokio::test]
    async fn check_if_campaign_is_sticky() {
        let mut manager = setup_manager("http://localhost:1337".to_string());

        // Case 1 - options has disabled sticky
        {
            manager.options.disabled_sticky = true;
            assert!(!manager.is_campaign_sticky(DUMMY_CAMPAIGN.id).await);
            manager.options.disabled_sticky = false;
        }
        // Case 2 - time is past stickiness treshold, less than 4 minutes ago
        {
            let history = vec![HistoryEntry {
                time: Utc::now() - Duration::days(1), // 24 hours ago
                unit_id: DUMMY_IPFS[0],
                campaign_id: DUMMY_CAMPAIGN.id,
                slot_id: DUMMY_IPFS[1],
            }];
            let history = Arc::new(RwLock::new(VecDeque::from(history)));

            manager.history = history;

            assert!(!manager.is_campaign_sticky(DUMMY_CAMPAIGN.id).await);
        }
        // Case 3 - time isn't past stickiness treshold, Utc::now()
        {
            let history = vec![HistoryEntry {
                time: Utc::now(),
                unit_id: DUMMY_IPFS[0],
                campaign_id: DUMMY_CAMPAIGN.id,
                slot_id: DUMMY_IPFS[1],
            }];
            let history = Arc::new(RwLock::new(VecDeque::from(history)));

            manager.history = history;

            assert!(manager.is_campaign_sticky(DUMMY_CAMPAIGN.id).await);
        }
    }

    #[tokio::test]
    async fn check_sticky_ad_unit() {
        let server = MockServer::start().await;
        let mut manager = setup_manager(server.uri());
        let history = vec![HistoryEntry {
            time: Utc::now(),
            unit_id: DUMMY_AD_UNITS[0].ipfs,
            campaign_id: DUMMY_CAMPAIGN.id,
            slot_id: manager.options.market_slot,
        }];
        let history = Arc::new(RwLock::new(VecDeque::from(history)));
        manager.history = history;

        let campaign = Campaign {
            campaign: DUMMY_CAMPAIGN.clone(),
            units_with_price: vec![UnitsWithPrice {
                unit: AdUnit::from(&DUMMY_AD_UNITS[0]),
                price: UnifiedNum::from_whole(0.0001),
            }],
        };
        let res = manager
            .get_sticky_ad_unit(&[campaign], "http://localhost:1337")
            .await;

        assert!(res.is_some());

        // TODO: Here we modify the campaign manually, verify that such a scenario is possible
        let campaign = Campaign {
            campaign: DUMMY_CAMPAIGN.clone(),
            units_with_price: vec![UnitsWithPrice {
                unit: AdUnit::from(&DUMMY_AD_UNITS[1]),
                price: UnifiedNum::from_whole(0.0001),
            }],
        };

        let res = manager
            .get_sticky_ad_unit(&[campaign], "http://localhost:1337")
            .await;

        assert!(res.is_none());
    }
}
