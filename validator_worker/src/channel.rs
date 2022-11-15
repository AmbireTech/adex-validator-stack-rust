use crate::{
    error::{Error, TickError},
    follower, leader, SentryApi,
};

use adapter::prelude::*;
use primitives::{config::Config, ChainOf, Channel, ChannelId};
use slog::info;
use tokio::time::timeout;

pub async fn channel_tick<C: Unlocked + 'static>(
    sentry: &SentryApi<C>,
    config: &Config,
    channel_context: ChainOf<Channel>,
) -> Result<(ChannelId, Box<dyn std::fmt::Debug>), Error> {
    let logger = sentry.logger.clone();
    let channel = channel_context.context;

    let adapter = &sentry.adapter;
    let tick = channel_context
        .context
        .find_validator(adapter.whoami())
        .ok_or_else(|| Error::ChannelNotIntendedForUs(channel.id(), adapter.whoami()))?;

    // 1. `GET /channel/:id/spender/all`
    let all_spenders = sentry.get_all_spenders(&channel_context).await?;

    // 2. `GET /channel/:id/accounting`
    // Validation #1:
    // sum(Accounting.spenders) == sum(Accounting.earners)
    let accounting = sentry.get_accounting(&channel_context).await?;

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

    match tick {
        primitives::Validator::Leader(_v) => match timeout(
            config.channel_tick_timeout,
            leader::tick(sentry, &channel_context, accounting.balances),
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
                follower::tick(sentry, &channel_context, all_spenders, accounting.balances);
            match timeout(config.channel_tick_timeout, follower_fut).await {
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
