use std::{collections::HashSet, sync::Arc};

use axum::{
    extract::{Path, TypedHeader},
    headers::UserAgent,
    http::header::{HeaderMap, HeaderName},
    Extension, Json,
};
use chrono::Utc;
use futures::future::try_join_all;
use once_cell::sync::Lazy;
use reqwest::Url;
use slog::{debug, error, Logger};
use thiserror::Error;
use woothee::{parser::Parser, woothee::VALUE_UNKNOWN};

use adapter::client::Locked;
use primitives::{
    sentry::{
        units_for_slot::{
            response::{self, Response},
            Query,
        },
        IMPRESSION,
    },
    targeting::{eval_with_callback, get_pricing_bounds, input, input::Input, Output},
    AdSlot, Address, Campaign, Config, UnifiedNum, ValidatorId, IPFS,
};

use crate::{
    application::Qs,
    db::{
        accounting::{get_accounting, Side},
        units_for_slot_get_campaigns, CampaignRemaining, DbPool, PoolError, RedisError,
    },
    response::ResponseError,
    Application,
};

pub(crate) static CLOUDFLARE_IPCOUNTRY_HEADER: Lazy<HeaderName> =
    Lazy::new(|| HeaderName::from_static("cf-ipcountry"));

#[cfg(test)]
#[path = "./units_for_slot_test.rs"]
pub mod test;

#[derive(Debug, Error)]
#[error(transparent)]
pub enum CampaignsError {
    Redis(#[from] RedisError),
    Postgres(#[from] PoolError),
}

pub async fn get_units_for_slot<C>(
    Extension(app): Extension<Arc<Application<C>>>,
    Path(ad_slot): Path<IPFS>,
    Qs(query): Qs<Query>,
    user_agent: Option<TypedHeader<UserAgent>>,
    headers: HeaderMap,
) -> Result<Json<Response>, ResponseError>
where
    C: Locked + 'static,
{
    let ad_slot_response = app
        .platform_api
        .fetch_slot(ad_slot)
        .await?
        .ok_or(ResponseError::NotFound)?;

    debug!(&app.logger, "Fetched AdSlot from the platform"; "AdSlotResponse" => ?&ad_slot_response);

    let slot = ad_slot_response.slot;
    let (accepted_referrers, categories) = match ad_slot_response.website {
        Some(website) => (website.accepted_referrers, website.categories),
        None => Default::default(),
    };

    // For each adUnits apply input
    let ua_parser = Parser::new();
    let user_agent = user_agent
        .map(|h| h.as_str().to_string())
        .unwrap_or_default();
    let parsed = ua_parser.parse(&user_agent);
    // WARNING! This will return only the OS type, e.g. `Linux` and not the actual distribution name e.g. `Ubuntu`
    // By contrast `ua-parser-js` will return `Ubuntu` (distribution) and not the OS type `Linux`.
    // `UAParser(...).os.name` (`ua-parser-js: 0.7.22`)
    let user_agent_os = parsed.as_ref().and_then(|p| {
        if p.os != VALUE_UNKNOWN {
            Some(p.os.to_string())
        } else {
            None
        }
    });

    // Corresponds to `UAParser(...).browser.name` (`ua-parser-js: 0.7.22`)
    let user_agent_browser_family = parsed.as_ref().and_then(|p| {
        if p.name != VALUE_UNKNOWN {
            Some(p.name.to_string())
        } else {
            None
        }
    });

    let country = headers
        .get(CLOUDFLARE_IPCOUNTRY_HEADER.clone())
        .and_then(|h| h.to_str().map(ToString::to_string).ok());

    let hostname = Url::parse(&slot.website.clone().unwrap_or_default())
        .ok()
        .and_then(|url| url.host().map(|h| h.to_string()))
        .unwrap_or_default();

    let publisher_id = slot.owner;

    let campaigns_limited_by_earner = get_campaigns(
        app.pool.clone(),
        app.campaign_remaining.clone(),
        &app.config,
        &Some(query.deposit_assets),
        publisher_id,
    )
    .await?;

    debug!(&app.logger, "Fetched Cache campaigns limited by earner (publisher)"; "campaigns" => campaigns_limited_by_earner.len(), "publisher_id" => %publisher_id);

    // We return those in the result (which means AdView would have those) but we don't actually use them
    // we do that in order to have the same variables as the validator, so that the `price` is the same
    let targeting_input_ad_slot = Some(input::AdSlot {
        categories: categories.clone().into_iter().collect(),
        hostname,
    });

    let mut targeting_input_base = Input {
        ad_view: None,
        global: input::Global {
            ad_slot_id: slot.ipfs,
            ad_slot_type: slot.ad_type.clone(),
            publisher_id: publisher_id.to_address(),
            country,
            event_type: IMPRESSION,
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
        &app.config,
        &app.logger,
        campaigns_limited_by_earner,
        targeting_input_base.clone(),
        slot,
    )
    .await;

    targeting_input_base.ad_slot = targeting_input_ad_slot;

    let response = Response {
        targeting_input_base,
        accepted_referrers,
        campaigns,
        fallback_unit: ad_slot_response
            .fallback
            .map(|ad_unit| response::AdUnit::from(&ad_unit)),
    };

    Ok(Json(response))
}

async fn get_campaigns(
    pool: DbPool,
    campaign_remaining: CampaignRemaining,
    config: &Config,
    deposit_assets: &Option<HashSet<Address>>,
    publisher_id: ValidatorId,
) -> Result<Vec<Campaign>, CampaignsError> {
    // 1. Fetch active Campaigns: (postgres)
    //  Creator = publisher_id
    // if deposit asset > 0 => filter by deposit_asset
    // TODO: limit the amount of passable deposit assets to avoid the max query size!
    let active_campaigns = units_for_slot_get_campaigns(
        &pool,
        deposit_assets.as_ref(),
        publisher_id.to_address(),
        Utc::now(),
    )
    .await?;

    let active_campaign_ids = &active_campaigns
        .iter()
        .map(|campaign| campaign.id)
        .collect::<Vec<_>>();

    // 2. Check those Campaigns if `Campaign remaining > 0` (in redis)
    let campaigns_remaining = campaign_remaining
        .get_multiple_with_ids(active_campaign_ids)
        .await?;

    let campaigns_with_remaining = campaigns_remaining
        .into_iter()
        .filter_map(|(campaign_id, remaining)| {
            // remaining should not be `0`
            if remaining > UnifiedNum::from(0) {
                // and we have to find the `Campaign` instance
                active_campaigns
                    .iter()
                    .find(|campaign| campaign.id == campaign_id)
                    .cloned()
            } else {
                None
            }
        })
        .collect::<Vec<_>>();

    let channels = campaigns_with_remaining
        .iter()
        .map(|campaign| campaign.channel.id())
        .collect::<HashSet<_>>();

    let publisher_accountings = try_join_all(channels.iter().map(|channel_id| {
        get_accounting(
            pool.clone(),
            *channel_id,
            publisher_id.to_address(),
            Side::Spender,
        )
    }))
    .await?
    .into_iter()
    .flatten()
    .collect::<Vec<_>>();

    // 3. Filter `Campaign`s, that include the `publisher_id` in the Channel balances.
    let (mut campaigns_by_earner, rest_of_campaigns): (Vec<Campaign>, Vec<Campaign>) =
        campaigns_with_remaining.into_iter().partition(|campaign| {
            publisher_accountings
                .iter()
                .any(|accounting| accounting.channel_id == campaign.channel.id())
        });

    let campaigns = if campaigns_by_earner.len()
        >= config
            .limits
            .units_for_slot
            .max_campaigns_earning_from
            .into()
    {
        campaigns_by_earner
    } else {
        campaigns_by_earner.extend(rest_of_campaigns.into_iter());

        campaigns_by_earner
    };

    Ok(campaigns)
}

async fn apply_targeting(
    config: &Config,
    logger: &Logger,
    campaigns: Vec<Campaign>,
    input_base: Input,
    ad_slot: AdSlot,
) -> Vec<response::Campaign> {
    campaigns
            .into_iter()
            .filter_map(|campaign| {
                let ad_units = campaign
                    .ad_units
                    .iter()
                    .filter(|ad_unit| ad_unit.ad_type == ad_slot.ad_type)
                    .cloned()
                    .collect::<Vec<_>>();

                if ad_units.is_empty() {
                    None
                } else {
                    let campaign_input = input_base.clone().with_campaign(campaign.clone());

                    let matching_units: Vec<response::UnitsWithPrice> = ad_units
                        .into_iter()
                        .filter_map(|ad_unit| {
                            let mut unit_input = campaign_input.clone();
                            unit_input.ad_unit_id = Some(ad_unit.ipfs);

                            let pricing_bounds = get_pricing_bounds(&campaign, &IMPRESSION);
                            let mut output = Output {
                                show: true,
                                boost: 1.0,
                                // only "IMPRESSION" event can be used for this `Output`
                                price: [(IMPRESSION, pricing_bounds.min)]
                                    .into_iter()
                                    .collect(),
                            };

                            let on_type_error_campaign = |error, rule| error!(logger, "Rule evaluation error for {:?}", campaign.id; "error" => ?error, "rule" => ?rule, "campaign" => ?campaign);
                            eval_with_callback(&campaign.targeting_rules, &unit_input, &mut output, Some(on_type_error_campaign));

                            if !output.show {
                                return None;
                            }

                            let max_price = match output.price.get(IMPRESSION.as_str()) {
                                Some(output_price) => *output_price.min(&pricing_bounds.max),
                                None => pricing_bounds.max,
                            };
                            let price = pricing_bounds.min.max(max_price);

                            if price < config.limits.units_for_slot.global_min_impression_price {
                                return None;
                            }

                            // Execute the adSlot rules after we've taken the price since they're not
                            // allowed to change the price
                            let on_type_error_adslot = |error, rule| error!(logger, "Rule evaluation error AdSlot {:?}", ad_slot.ipfs; "error" => ?error, "rule" => ?rule);

                            eval_with_callback(&ad_slot.rules, &unit_input, &mut output, Some(on_type_error_adslot));
                            if !output.show {
                                return None;
                            }

                            let ad_unit = response::AdUnit::from(&ad_unit);

                            Some(response::UnitsWithPrice {
                                unit: ad_unit,
                                price,
                            })
                        })
                        .collect();

                    if matching_units.is_empty() {
                        None
                    } else {
                        Some(response::Campaign {
                            campaign,
                            units_with_price: matching_units,
                        })
                    }
                }
            })
            .collect()
}
