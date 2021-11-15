use crate::{
    error::{Error, TickError},
    follower, leader,
    sentry_interface::{campaigns::all_campaigns, Validator, Validators},
    SentryApi,
};
use primitives::{adapter::Adapter, config::Config, util::ApiUrl, Channel, ChannelId};
use slog::{info, Logger};
use std::{
    collections::{hash_map::Entry, HashSet},
    time::Duration,
};
use tokio::time::timeout;

pub async fn channel_tick<A: Adapter + 'static>(
    sentry: &SentryApi<A>,
    config: &Config,
    channel: Channel,
) -> Result<(ChannelId, Box<dyn std::fmt::Debug>), Error> {
    let logger = sentry.logger.clone();

    let adapter = &sentry.adapter;
    let tick = channel
        .find_validator(adapter.whoami())
        .ok_or(Error::ChannelNotIntendedForUs)?;

    // 1. `GET /channel/:id/spender/all`
    let all_spenders = sentry.get_all_spenders(channel.id()).await?;

    // 2. `GET /channel/:id/accounting`
    // Validation #1:
    // sum(Accounting.spenders) == sum(Accounting.earners)
    let accounting = sentry.get_accounting(channel.id()).await?;

    // Validation #2:
    // spender.total_deposit >= accounting.balances.spenders[spender.address]
    if !all_spenders.iter().all(|(address, spender)| {
        spender.total_deposited
            >= accounting
                .balances
                .spenders
                .get(address)
                .cloned()
                .unwrap_or_default()
    }) {
        return Err(Error::Validation);
    }

    let token = config
        .token_address_whitelist
        .get(&channel.token)
        .ok_or(Error::ChannelTokenNotWhitelisted)?;

    let duration = Duration::from_millis(config.channel_tick_timeout as u64);

    match tick {
        primitives::Validator::Leader(_v) => match timeout(
            duration,
            leader::tick(sentry, channel, accounting.balances, token),
        )
        .await
        {
            Err(timeout_e) => Err(Error::LeaderTick(
                channel.id(),
                TickError::TimedOut(timeout_e),
            )),
            Ok(Err(tick_e)) => Err(Error::LeaderTick(
                channel.id(),
                TickError::Tick(Box::new(tick_e)),
            )),
            Ok(Ok(tick_status)) => {
                info!(&logger, "Leader tick"; "status" => ?tick_status);
                Ok((channel.id(), Box::new(tick_status)))
            }
        },
        primitives::Validator::Follower(_v) => {
            let follower_fut =
                follower::tick(sentry, channel, all_spenders, accounting.balances, token);
            match timeout(duration, follower_fut).await {
                Err(timeout_e) => Err(Error::FollowerTick(
                    channel.id(),
                    TickError::TimedOut(timeout_e),
                )),
                Ok(Err(tick_e)) => Err(Error::FollowerTick(
                    channel.id(),
                    TickError::Tick(Box::new(tick_e)),
                )),
                Ok(Ok(tick_status)) => {
                    info!(&logger, "Follower tick"; "status" => ?tick_status);
                    Ok((channel.id(), Box::new(tick_status)))
                }
            }
        }
    }
}

/// Fetches all `Campaign`s from Sentry and builds the `Channel`s to be processed
/// along side all the `Validator`s' url & auth token
// TODO: Move to [`SentryApi`]
pub async fn collect_channels<A: Adapter + 'static>(
    adapter: &A,
    sentry_url: &ApiUrl,
    config: &Config,
    _logger: &Logger,
) -> Result<(HashSet<Channel>, Validators), reqwest::Error> {
    let for_whoami = adapter.whoami();

    let all_campaigns_timeout = Duration::from_millis(config.all_campaigns_timeout as u64);
    let client = reqwest::Client::builder()
        .timeout(all_campaigns_timeout)
        .build()?;

    let whoami_validator = Validator {
        url: sentry_url.clone(),
        token: adapter
            .get_auth(&for_whoami)
            .expect("Should get WhoAmI auth"),
    };
    let campaigns = all_campaigns(client, &whoami_validator, Some(for_whoami)).await?;
    let channels = campaigns
        .iter()
        .map(|campaign| campaign.channel)
        .collect::<HashSet<_>>();

    let validators = campaigns
        .into_iter()
        .fold(Validators::new(), |mut acc, campaign| {
            for validator_desc in campaign.validators.iter() {
                // if Validator is already there, we can just skip it
                // remember, the campaigns are ordered by `created DESC`
                // so we will always get the latest Validator url first
                match acc.entry(validator_desc.id) {
                    Entry::Occupied(_) => continue,
                    Entry::Vacant(entry) => {
                        // try to parse the url of the Validator Desc
                        let validator_url = validator_desc.url.parse::<ApiUrl>();
                        // and also try to find the Auth token in the config

                        // if there was an error with any of the operations, skip this `ValidatorDesc`
                        let auth_token = adapter.get_auth(&validator_desc.id);

                        // only if `ApiUrl` parsing is `Ok` & Auth Token is found in the `Adapter`
                        if let (Ok(url), Ok(auth_token)) = (validator_url, auth_token) {
                            // add an entry for propagation
                            entry.insert(Validator {
                                url,
                                token: auth_token,
                            });
                        }
                        // otherwise it will try to do the same things on the next encounter of this `ValidatorId`
                    }
                }
            }

            acc
        });

    Ok((channels, validators))
}
