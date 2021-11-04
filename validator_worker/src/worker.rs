use crate::{
    channel::{channel_tick, collect_channels},
    SentryApi,
};
use primitives::{adapter::Adapter, util::ApiUrl, Config};
use slog::{error, info, Logger};
use std::{error::Error, time::Duration};

use futures::{
    future::{join, join_all},
    TryFutureExt,
};
use tokio::{runtime::Runtime, time::sleep};

#[derive(Debug, Clone)]
pub struct Worker<A: Adapter> {
    pub sentry_url: ApiUrl,
    pub config: Config,
    pub adapter: A,
    pub logger: Logger,
}

impl<A: Adapter + 'static> Worker<A> {
    /// Runs the validator in a single tick or it runs infinitely.
    /// Before running, unlocks the adapter using [`Adapter::unlock`]!
    /// Uses [`tokio::runtime::Runtime`]
    pub fn run(&mut self, is_single_tick: bool) -> Result<(), Box<dyn Error>> {
        // unlock adapter
        self.adapter.unlock()?;

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
            let wait_time_future = sleep(Duration::from_millis(self.config.wait_time as u64));

            let _result = join(self.all_channels_tick(), wait_time_future).await;
        }
    }

    pub async fn all_channels_tick(&self) {
        let logger = &self.logger;
        let (channels, validators) = match collect_channels(
            &self.adapter,
            &self.sentry_url,
            &self.config,
            logger,
        )
        .await
        {
            Ok(res) => res,
            Err(err) => {
                error!(logger, "Error collecting all channels for tick"; "collect_channels" => ?err, "main" => "all_channels_tick");
                return;
            }
        };
        let channels_size = channels.len();

        // initialize SentryApi once we have all the Campaign Validators we need to propagate messages to
        let sentry = match SentryApi::init(
            self.adapter.clone(),
            logger.clone(),
            self.config.clone(),
            validators.clone(),
        ) {
            Ok(sentry) => sentry,
            Err(err) => {
                error!(logger, "Failed to initialize SentryApi for all channels"; "SentryApi::init()" => ?err, "main" => "all_channels_tick");
                return;
            }
        };

        let tick_results = join_all(channels.into_iter().map(|channel| {
            channel_tick(&sentry, &self.config, channel).map_err(move |err| (channel, err))
        }))
        .await;

        for (channel, channel_err) in tick_results.into_iter().filter_map(Result::err) {
            error!(logger, "Error processing Channel"; "channel" => ?channel, "error" => ?channel_err, "main" => "all_channels_tick");
        }

        info!(logger, "Processed {} channels", channels_size);

        if channels_size >= self.config.max_channels as usize {
            error!(logger, "WARNING: channel limit cfg.MAX_CHANNELS={} reached", &self.config.max_channels; "main" => "all_channels_tick");
        }
    }
}
