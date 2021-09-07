use crate::{
    error::Error,
    follower::TickStatus,
    sentry_interface::{campaigns::all_campaigns, Validator, Validators},
    SentryApi,
};
use primitives::{
    adapter::Adapter, channel_v5::Channel, config::Config, util::ApiUrl, ChannelId, UnifiedNum,
};
use slog::{error, info, Logger};
use std::collections::{hash_map::Entry, HashSet};

pub enum TickError {
    Validation,
}

async fn channel_tick<A: Adapter + 'static>(
    adapter: A,
    config: &Config,
    logger: &Logger,
    channel: Channel,
    validators: Validators,
) -> Result<ChannelId, Error<A::AdapterError>> {
    let sentry = SentryApi::init(
        adapter,
        logger.clone(),
        config.clone(),
        (channel, validators),
    )?;
    // `GET /channel/:id/spender/all`
    let all_spenders = sentry.get_all_spenders().await?;

    // `GET /channel/:id/accounting`
    // Validation #1:
    // sum(Accounting.spenders) == sum(Accounting.earners)
    let accounting = sentry.get_accounting(channel.id()).await?;

    // Validation #2:
    // spender.spender_leaf.total_deposit >= accounting.balances.spenders[spender.address]
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

    // Get Last Approved State & Last Approved NewState
    let last_approve_state = sentry.get_last_approved(channel.id()).await?;
    let new_state_balances = last_approve_state
        .last_approved
        .and_then(|last_approved| last_approved.new_state)
        .map(|new_state_msg| new_state_msg.msg.into_inner().balances)
        .unwrap_or_default();

    // Validation #3
    // Accounting.balances != NewState.balances
    

    // Validation #4
    // OUTPACE Rules:
    let (accounting_spenders, accounting_earners) = (
        accounting
            .balances
            .spenders
            .values()
            .sum::<Option<UnifiedNum>>()
            .ok_or(Error::Overflow)?,
        accounting
            .balances
            .earners
            .values()
            .sum::<Option<UnifiedNum>>()
            .ok_or(Error::Overflow)?,
    );
    let (new_state_spenders, new_state_earners) = (
        new_state_balances
            .spenders
            .values()
            .sum::<Option<UnifiedNum>>()
            .ok_or(Error::Overflow)?,
        new_state_balances
            .earners
            .values()
            .sum::<Option<UnifiedNum>>()
            .ok_or(Error::Overflow)?,
    );
    // sum(accounting.balances.spenders) > sum(new_state.balances.spenders)
    // sum(accounting.balances.earners) > sum(new_state.balances.earners)
    if !(accounting_spenders > new_state_spenders) || !(accounting_earners > new_state_earners) {
        return Err(Error::Validation);
    }

    Ok(channel.id())
}

/// Fetches all `Campaign`s from Sentry and builds the `Channel`s to be processed
/// along side all the `Validator`s' url & auth token
async fn collect_channels<A: Adapter + 'static>(
    adapter: A,
    sentry_url: &ApiUrl,
    config: &Config,
    logger: &Logger,
) -> Result<(HashSet<Channel>, Validators), reqwest::Error> {
    let whoami = adapter.whoami();

    let campaigns = all_campaigns(sentry_url, whoami).await?;
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
