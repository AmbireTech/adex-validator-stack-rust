use crate::{
    error::{Error, TickError},
    follower, leader, SentryApi,
};

use adapter::prelude::*;
use primitives::{config::Config, Channel, ChannelId};
use slog::info;
use std::time::Duration;
use tokio::time::timeout;

pub async fn channel_tick<C: Unlocked + 'static>(
    sentry: &SentryApi<C>,
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
