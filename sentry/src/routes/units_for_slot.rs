use std::collections::HashSet;

use adapter::client::Locked;
// use crate::{
//     cache::{
//         campaign::{Cache, Campaign, Client},
//         market::{
//             ad_slot::AdSlotOutput,
//             ad_unit::{AdTypeOutput, AdUnitsOutput},
//             CacheLike, ClientLike,
//         },
//         Caches,
//     },
//     not_found, service_unavailable,
//     status::Status,
//     Config, Error, ROUTE_UNITS_FOR_SLOT,
// };
use chrono::Utc;
use hyper::{header::USER_AGENT, Body, Request, Response};
use hyper::{
    header::{HeaderName, CONTENT_TYPE},
    StatusCode,
};
use once_cell::sync::Lazy;
use primitives::{
    market::AdSlotResponse,
    supermarket::units_for_slot::response,
    supermarket::units_for_slot::response::Response as UnitsForSlotResponse,
    targeting::{eval_with_callback, get_pricing_bounds, input, input::Input, Output},
    AdSlot, AdUnit, Address, Campaign, Config, ValidatorId, IPFS,
};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use slog::{debug, error, warn, Logger};
// use url::{form_urlencoded, Url};
use woothee::{parser::Parser, woothee::VALUE_UNKNOWN};

use platform::PlatformApi;

use crate::{Application, ResponseError};

pub(crate) static CLOUDFLARE_IPCOUNTY_HEADER: Lazy<HeaderName> =
    Lazy::new(|| HeaderName::from_static("cf-ipcountry"));

// #[cfg(test)]
// #[path = "units_for_slot_test.rs"]
// pub mod test;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RequestBody {
    ad_slot: AdSlot,
    deposit_assets: HashSet<Address>,
}

pub(crate) fn not_found() -> Response<Body> {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .body(Body::empty())
        .expect("Not Found response should be valid")
}

pub(crate) fn service_unavailable() -> Response<Body> {
    Response::builder()
        .status(StatusCode::SERVICE_UNAVAILABLE)
        .body(Body::empty())
        .expect("Bad Request response should be valid")
}

pub async fn post_units_for_slot<C>(
    req: Request<Body>,
    app: &Application<C>,
    platform: PlatformApi,
    // ipfs: IPFS,
    // caches: Caches<C, AU, AT, AS, E>,
) -> Result<Response<Body>, ResponseError>
where
    C: Locked + 'static,
{
    let logger = &app.logger;
    let config = &app.config;

    let (request_parts, body) = req.into_parts();

    let body_bytes = hyper::body::to_bytes(body).await?;

    let request_body = serde_json::from_slice::<RequestBody>(&body_bytes)?;

    let ad_slot = request_body.ad_slot.clone();

    // TODO: remove once we know how/where we will be fetching the rest of the information!
    let ad_slot_response = AdSlotResponse {
        slot: request_body.ad_slot.clone(),
        accepted_referrers: vec!["TODO".parse().unwrap()],
        categories: vec!["TODO".into()],
        alexa_rank: Some(1.0),
    };

    let units = match platform.fetch_units(&request_body.ad_slot.ad_type).await {
        Ok(units) => units,
        Err(error) => {
            error!(&logger, "Error fetching AdUnits for AdSlot"; "AdSlot" => ?ad_slot, "error" => ?error);

            return Ok(service_unavailable());
        }
    };

    let accepted_referrers = ad_slot_response.accepted_referrers.clone();
    let fallback_unit: Option<AdUnit> = match &ad_slot_response.slot.fallback_unit {
        Some(unit_ipfs) => {
            let ipfs = unit_ipfs.parse::<IPFS>()?;
            let ad_unit_response = match platform.fetch_unit(ipfs.clone()).await {
                Ok(Some(response)) => {
                    debug!(&logger, "Fetched AdUnit"; "AdUnit" => &ipfs);
                    response
                }
                Ok(None) => {
                    warn!(
                        &logger,
                        "AdSlot fallback AdUnit ({}) not found in Platform",
                        &ipfs;
                        "AdUnit" => &ipfs,
                        "AdSlot" => ?ad_slot,
                    );

                    return Ok(not_found());
                }
                Err(error) => {
                    error!(&logger,
                        "Error when fetching AdSlot fallback AdUnit ({}) from Platform",
                        unit_ipfs;
                        "AdSlot" => ?ad_slot,
                        "Fallback AdUnit" => unit_ipfs,
                        "error" => ?error
                    );

                    return Ok(service_unavailable());
                }
            };

            Some(ad_unit_response.unit)
        }
        None => None,
    };

    debug!(&logger, "Fetched {} AdUnits for AdSlot", units.len(); "AdSlot" => ?ad_slot);
    // let query = req.uri().query().unwrap_or_default();
    // let parsed_query = form_urlencoded::parse(query.as_bytes());

    // For each adUnits apply input
    let ua_parser = Parser::new();
    let user_agent = request_parts
        .headers
        .get(USER_AGENT)
        .and_then(|h| h.to_str().map(ToString::to_string).ok())
        .unwrap_or_default();
    let parsed = ua_parser.parse(&user_agent);
    // WARNING! This will return only the OS type, e.g. `Linux` and not the actual distribution name e.g. `Ubuntu`
    // By contrast `ua-parser-js` will return `Ubuntu` (distribution) and not the OS type `Linux`.
    // `UAParser(...).os.name` (`ua-parser-js: 0.7.22`)
    let user_agent_os = parsed
        .as_ref()
        .map(|p| {
            if p.os != VALUE_UNKNOWN {
                Some(p.os.to_string())
            } else {
                None
            }
        })
        .flatten();

    // Corresponds to `UAParser(...).browser.name` (`ua-parser-js: 0.7.22`)
    let user_agent_browser_family = parsed
        .as_ref()
        .map(|p| {
            if p.name != VALUE_UNKNOWN {
                Some(p.name.to_string())
            } else {
                None
            }
        })
        .flatten();

    let country = request_parts
        .headers
        .get(CLOUDFLARE_IPCOUNTY_HEADER.clone())
        .and_then(|h| h.to_str().map(ToString::to_string).ok());

    let hostname = Url::parse(&ad_slot.website.clone().unwrap_or_default())
        .ok()
        .and_then(|url| url.host().map(|h| h.to_string()))
        .unwrap_or_default();

    let publisher_id = ad_slot_response.slot.owner;

    let campaigns_limited_by_earner = get_campaigns(
        /* &caches.campaigns, */ config,
        &request_body.deposit_assets,
        publisher_id,
    )
    .await;

    debug!(&logger, "Fetched Cache campaigns limited by earner (publisher)"; "campaigns" => campaigns_limited_by_earner.len(), "publisher_id" => %publisher_id);

    // We return those in the result (which means AdView would have those) but we don't actually use them
    // we do that in order to have the same variables as the validator, so that the `price` is the same
    let targeting_input_ad_slot = Some(input::AdSlot {
        categories: ad_slot_response.categories.clone(),
        hostname,
        alexa_rank: ad_slot_response.alexa_rank,
    });

    let mut targeting_input_base = Input {
        ad_view: None,
        global: input::Global {
            ad_slot_id: ad_slot_response.slot.ipfs.clone(),
            ad_slot_type: ad_slot_response.slot.ad_type.clone(),
            publisher_id: publisher_id.to_address(),
            country,
            event_type: "IMPRESSION".to_string(),
            seconds_since_epoch: Utc::now(),
            user_agent_os,
            user_agent_browser_family: user_agent_browser_family.clone(),
        },
        ad_unit_id: None,
        balances: None,
        campaign: None,
        ad_slot: None,
    };

    let campaigns = apply_targeting(
        config,
        logger,
        campaigns_limited_by_earner,
        targeting_input_base.clone(),
        ad_slot_response,
    )
    .await;

    targeting_input_base.ad_slot = targeting_input_ad_slot;

    let response = UnitsForSlotResponse {
        targeting_input_base,
        accepted_referrers,
        campaigns,
        fallback_unit: fallback_unit.map(|ad_unit| response::AdUnit::from(&ad_unit)),
    };

    Ok(Response::builder()
        .status(StatusCode::OK)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_string(&response)?))
        .expect("Should create response"))
    // }
}

async fn get_campaigns(
    // cache: &Cache<C>,
    config: &Config,
    deposit_assets: &HashSet<Address>,
    publisher_id: ValidatorId,
) -> Vec<Campaign> {
    todo!()
    //     let active_campaigns = cache.active.read().await;

    //     let (mut campaigns_by_earner, rest_of_campaigns): (Vec<&Campaign>, Vec<&Campaign>) =
    //         active_campaigns
    //             .iter()
    //             .filter_map(|(_, campaign)| {
    //                 // The Supermarket has the Active status combining Active & Ready from Market
    //                 if campaign.status == Status::Active
    //                     && campaign.channel.creator != publisher_id
    //                     && (deposit_assets.is_empty()
    //                         || deposit_assets.contains(&campaign.channel.deposit_asset))
    //                 {
    //                     Some(campaign)
    //                 } else {
    //                     None
    //                 }
    //             })
    //             .partition(|&campaign| campaign.balances.contains_key(&publisher_id));

    //     if campaigns_by_earner.len() >= config.limits.max_channels_earning_from.into() {
    //         campaigns_by_earner.into_iter().cloned().collect()
    //     } else {
    //         campaigns_by_earner.extend(rest_of_campaigns.iter());

    //         campaigns_by_earner.into_iter().cloned().collect()
    //     }
}

async fn apply_targeting(
    config: &Config,
    logger: &Logger,
    campaigns: Vec<Campaign>,
    input_base: Input,
    ad_slot_response: AdSlotResponse,
) -> Vec<response::Campaign> {
    todo!()
    //     campaigns
    //         .into_iter()
    //         .filter_map::<response::Campaign, _>(|campaign| {
    //             let ad_units = campaign
    //                 .channel
    //                 .spec
    //                 .ad_units
    //                 .iter()
    //                 .filter(|ad_unit| ad_unit.ad_type == ad_slot_response.slot.ad_type)
    //                 .cloned()
    //                 .collect::<Vec<_>>();

    //             if ad_units.is_empty() {
    //                 None
    //             } else {
    //                 let targeting_rules = if !campaign.channel.targeting_rules.is_empty() {
    //                     campaign.channel.targeting_rules.clone()
    //                 } else {
    //                     campaign.channel.spec.targeting_rules.clone()
    //                 };
    //                 let campaign_input = input_base.clone().with_channel(campaign.channel.clone());

    //                 let matching_units: Vec<response::UnitsWithPrice> = ad_units
    //                     .into_iter()
    //                     .filter_map(|ad_unit| {
    //                         let mut unit_input = campaign_input.clone();
    //                         unit_input.ad_unit_id = Some(ad_unit.ipfs.clone());

    //                         let pricing_bounds = get_pricing_bounds(&campaign.channel, "IMPRESSION");
    //                         let mut output = Output {
    //                             show: true,
    //                             boost: 1.0,
    //                             // only "IMPRESSION" event can be used for this `Output`
    //                             price: vec![("IMPRESSION".to_string(), pricing_bounds.min.clone())]
    //                                 .into_iter()
    //                                 .collect(),
    //                         };

    //                         let on_type_error_campaign = |error, rule| error!(logger, "Rule evaluation error for {:?}", campaign.channel.id; "error" => ?error, "rule" => ?rule);
    //                         eval_with_callback(&targeting_rules, &unit_input, &mut output, Some(on_type_error_campaign));

    //                         if !output.show {
    //                             return None;
    //                         }

    //                         let max_price = match output.price.get("IMPRESSION") {
    //                             Some(output_price) => output_price.min(&pricing_bounds.max).clone(),
    //                             None => pricing_bounds.max,
    //                         };
    //                         let price = pricing_bounds.min.max(max_price);

    //                         if price < config.limits.global_min_impression_price {
    //                             return None;
    //                         }

    //                         // Execute the adSlot rules after we've taken the price since they're not
    //                         // allowed to change the price
    //                         let on_type_error_adslot = |error, rule| error!(logger, "Rule evaluation error AdSlot {:?}", ad_slot_response.slot.ipfs; "error" => ?error, "rule" => ?rule);

    //                         eval_with_callback(&ad_slot_response.slot.rules, &unit_input, &mut output, Some(on_type_error_adslot));
    //                         if !output.show {
    //                             return None;
    //                         }

    //                         let ad_unit = response::AdUnit::from(&ad_unit);

    //                         Some(response::UnitsWithPrice {
    //                             unit: ad_unit,
    //                             price,
    //                         })
    //                     })
    //                     .collect();

    //                 if matching_units.is_empty() {
    //                     None
    //                 } else {
    //                     Some(response::Campaign {
    //                         channel: campaign.channel.into(),
    //                         targeting_rules,
    //                         units_with_price: matching_units,
    //                     })
    //                 }
    //             }
    //         })
    //         .collect()
}

/// previously fetched from the market (in the supermarket) it should now be fetched from the Platform!
mod platform {
    use primitives::{
        market::{AdSlotResponse, AdUnitResponse, AdUnitsResponse, Campaign, StatusType},
        util::ApiUrl,
        AdUnit, IPFS,
    };
    use reqwest::{Client, Error, StatusCode};
    use slog::{info, Logger};
    use std::{fmt, sync::Arc};

    use crate::Config;

    pub type Result<T> = std::result::Result<T, Error>;

    #[derive(Debug, Clone)]
    /// The `PlatformApi` is cheap to clone as it already wraps the real client `PlatformApiInner` in an `Arc`
    pub struct PlatformApi {
        inner: Arc<PlatformApiInner>,
    }

    impl PlatformApi {
        /// The Market url that was is used for communication with the API
        pub fn url(&self) -> &ApiUrl {
            &self.inner.platform_url
        }

        // todo: Instead of associate function, use a builder
        pub fn new(platform_url: ApiUrl, config: &Config, logger: Logger) -> Result<Self> {
            Ok(Self {
                inner: Arc::new(PlatformApiInner::new(platform_url, config, logger)?),
            })
        }

        pub async fn fetch_unit(&self, ipfs: IPFS) -> Result<Option<AdUnitResponse>> {
            self.inner.fetch_unit(ipfs).await
        }

        pub async fn fetch_units(&self, ad_type: &str) -> Result<Vec<AdUnit>> {
            self.inner.fetch_units(ad_type).await
        }
    }

    /// Should we query All or only certain statuses
    #[derive(Debug)]
    pub enum Statuses<'a> {
        All,
        Only(&'a [StatusType]),
    }

    impl fmt::Display for Statuses<'_> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            use Statuses::*;

            match self {
                All => write!(f, "all"),
                Only(statuses) => {
                    let statuses = statuses.iter().map(ToString::to_string).collect::<Vec<_>>();

                    write!(f, "status={}", statuses.join(","))
                }
            }
        }
    }

    #[derive(Debug, Clone)]
    struct PlatformApiInner {
        platform_url: ApiUrl,
        client: Client,
        logger: Logger,
    }

    impl PlatformApiInner {
        /// The limit of Campaigns per page when fetching
        /// Limit the value to MAX(500)
        const MARKET_CAMPAIGNS_LIMIT: u64 = 500;
        /// The limit of AdUnits per page when fetching
        /// It should always be > 1
        const MARKET_AD_UNITS_LIMIT: u64 = 1_000;

        pub fn new(platform_url: ApiUrl, config: &Config, logger: Logger) -> Result<Self> {
            // @TODO: maybe add timeout?
            let client = Client::builder()
                // TODO: Move this config value from Supermarket if needed
                // .tcp_keepalive(config.market.keep_alive_interval)
                .cookie_store(true)
                .build()?;

            Ok(Self {
                platform_url,
                client,
                logger,
            })
        }

        pub async fn fetch_unit(&self, ipfs: IPFS) -> Result<Option<AdUnitResponse>> {
            let url = self
                .platform_url
                .join(&format!("units/{}", ipfs))
                .expect("Wrong Platform Url for /units/{IPFS} endpoint");

            match self.client.get(url).send().await?.error_for_status() {
                Ok(response) => {
                    let ad_unit_response = response.json::<AdUnitResponse>().await?;

                    Ok(Some(ad_unit_response))
                }
                // if we have a `404 Not Found` error, return None
                Err(err) if err.status() == Some(StatusCode::NOT_FOUND) => Ok(None),
                Err(err) => Err(err),
            }
        }

        pub async fn fetch_units(&self, ad_type: &str) -> Result<Vec<AdUnit>> {
            let mut units = Vec::new();
            let mut skip: u64 = 0;
            let limit = Self::MARKET_AD_UNITS_LIMIT;

            loop {
                // if one page fail, simply return the error for now
                let mut page_results = self.fetch_units_page(ad_type, skip).await?;
                // get the count before appending the page results to all
                let count = page_results.len() as u64;

                // append all received units
                units.append(&mut page_results);
                // add the number of results we need to skip in the next iteration
                skip += count;

                // if the Market returns < market fetch limit
                // we've got all AdSlots from all pages!
                if count < limit {
                    // so break out of the loop
                    break;
                }
            }

            Ok(units)
        }

        /// `skip` - how many records it should skip (pagination)
        async fn fetch_units_page(&self, ad_type: &str, skip: u64) -> Result<Vec<AdUnit>> {
            let url = self
                .platform_url
                .join(&format!(
                    "units?limit={}&skip={}&type={}",
                    Self::MARKET_AD_UNITS_LIMIT,
                    skip,
                    ad_type,
                ))
                .expect("Wrong Market Url for /units endpoint");

            let response = self.client.get(url).send().await?;

            let ad_units: AdUnitsResponse = response.json().await?;

            Ok(ad_units.0)
        }
    }
}
