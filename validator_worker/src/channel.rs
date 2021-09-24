use crate::{
    error::{Error, TickError},
    follower, leader,
    sentry_interface::{campaigns::all_campaigns, Validator, Validators},
    SentryApi,
};
use primitives::{adapter::Adapter, channel::Channel, config::Config, util::ApiUrl, ChannelId};
use slog::Logger;
use std::collections::{hash_map::Entry, HashSet};

pub async fn channel_tick<A: Adapter + 'static>(
    sentry: &SentryApi<A>,
    config: &Config,
    channel: Channel,
    // validators: &Validators,
) -> Result<ChannelId, Error> {
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

    // TODO: Add timeout
    match tick {
        primitives::Validator::Leader(_v) => {
            let _leader_tick_status = leader::tick(sentry, channel, accounting.balances, token)
                .await
                .map_err(|err| Error::LeaderTick(channel.id(), TickError::Tick(Box::new(err))))?;
        }
        primitives::Validator::Follower(_v) => {
            let _follower_tick_status =
                follower::tick(sentry, channel, all_spenders, accounting.balances, token)
                    .await
                    .map_err(|err| {
                        Error::FollowerTick(channel.id(), TickError::Tick(Box::new(err)))
                    })?;
        }
    };

    Ok(channel.id())
}

/// Fetches all `Campaign`s from Sentry and builds the `Channel`s to be processed
/// along side all the `Validator`s' url & auth token
pub async fn collect_channels<A: Adapter + 'static>(
    adapter: &A,
    sentry_url: &ApiUrl,
    _config: &Config,
    _logger: &Logger,
) -> Result<(HashSet<Channel>, Validators), reqwest::Error> {
    let whoami = adapter.whoami();

    // TODO: Move client creation
    let client = reqwest::Client::new();
    let campaigns = all_campaigns(client, sentry_url, whoami).await?;
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
