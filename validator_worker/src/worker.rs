use crate::{channel::channel_tick, SentryApi};
use adapter::{prelude::*, Adapter};
use primitives::Config;
use slog::{error, info, Logger};
use std::error::Error;

use futures::{
    future::{join, join_all},
    TryFutureExt,
};
use tokio::{runtime::Runtime, time::sleep};

#[derive(Debug, Clone)]
pub struct Worker<C: Unlocked> {
    /// SentryApi with set `whoami` validator
    /// Requires an unlocked adapter to create [`SentryApi`], use `Worker::init_unlock()`.
    pub sentry: SentryApi<C, ()>,
    pub config: Config,
    /// The unlocked Adapter
    pub adapter: Adapter<C, UnlockedState>,
    pub logger: Logger,
}

impl<C: Unlocked + 'static> Worker<C> {
    /// Requires an unlocked [`Adapter`]
    pub fn from_sentry(sentry: SentryApi<C, ()>) -> Self {
        Self {
            config: sentry.config.clone(),
            adapter: sentry.adapter.clone(),
            logger: sentry.logger.clone(),
            sentry,
        }
    }

    /// Runs the validator in a single tick or it runs infinitely.
    /// Uses [`tokio::runtime::Runtime`]
    pub fn run(self, is_single_tick: bool) -> Result<(), Box<dyn Error>> {
        // Create the runtime
        let rt = Runtime::new()?;

        if is_single_tick {
            rt.block_on(self.all_channels_tick());
        } else {
            rt.block_on(self.infinite());
        }

        Ok(())
    }

    pub async fn infinite(&self) {
        loop {
            let wait_time_future = sleep(self.config.worker.wait_time);

            let _result = join(self.all_channels_tick(), wait_time_future).await;
        }
    }

    pub async fn all_channels_tick(&self) {
        let logger = &self.logger;

        let (channels_context, validators) = match self.sentry.collect_channels().await {
            Ok(res) => res,
            Err(err) => {
                error!(logger, "Error collecting all channels for tick"; "collect_channels" => ?err, "main" => "all_channels_tick");
                return;
            }
        };
        let channels_size = channels_context.len();

        let sentry_with_propagate = match self.sentry.clone().with_propagate(validators) {
            Ok(sentry) => sentry,
            Err(err) => {
                error!(logger, "Failed to set propagation validators: {err}"; "err" => ?err, "main" => "all_channels_tick");
                return;
            }
        };

        let tick_results = join_all(channels_context.into_iter().map(|channel_context| {
            let channel = channel_context.context;

            channel_tick(&sentry_with_propagate, &self.config, channel_context)
                .map_err(move |err| (channel, err))
        }))
        .await;

        for (channel, channel_err) in tick_results.into_iter().filter_map(Result::err) {
            error!(logger, "Error processing Channel"; "channel" => ?channel, "error" => ?channel_err, "main" => "all_channels_tick");
        }

        info!(logger, "Processed {} channels", channels_size);

        if channels_size >= self.config.worker.max_channels as usize {
            error!(logger, "WARNING: channel limit cfg.MAX_CHANNELS={} reached", &self.config.worker.max_channels; "main" => "all_channels_tick");
        }
    }
}
