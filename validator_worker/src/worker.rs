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
pub struct Args<A: Adapter> {
    sentry_url: ApiUrl,
    config: Config,
    adapter: A,
}

pub fn run<A: Adapter + 'static>(
    is_single_tick: bool,
    sentry_url: ApiUrl,
    config: &Config,
    mut adapter: A,
    logger: &Logger,
) -> Result<(), Box<dyn Error>> {
    // unlock adapter
    adapter.unlock()?;

    let args = Args {
        sentry_url,
        config: config.to_owned(),
        adapter,
    };

    // Create the runtime
    let rt = Runtime::new()?;

    if is_single_tick {
        rt.block_on(all_channels_tick(args, logger));
    } else {
        rt.block_on(infinite(args, logger));
    }

    Ok(())
}

pub async fn infinite<A: Adapter + 'static>(args: Args<A>, logger: &Logger) {
    loop {
        let arg = args.clone();
        let wait_time_future = sleep(Duration::from_millis(arg.config.wait_time as u64));

        let _result = join(all_channels_tick(arg, logger), wait_time_future).await;
    }
}

pub async fn all_channels_tick<A: Adapter + 'static>(args: Args<A>, logger: &Logger) {
    let (channels, validators) = match collect_channels(
        &args.adapter,
        &args.sentry_url,
        &args.config,
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
        args.adapter.clone(),
        logger.clone(),
        args.config.clone(),
        validators.clone(),
    ) {
        Ok(sentry) => sentry,
        Err(err) => {
            error!(logger, "Failed to initialize SentryApi for all channels"; "SentryApi::init()" => ?err, "main" => "all_channels_tick");
            return;
        }
    };

    let tick_results = join_all(channels.into_iter().map(|channel| {
        channel_tick(&sentry, &args.config, channel).map_err(move |err| (channel, err))
    }))
    .await;

    for (channel, channel_err) in tick_results.into_iter().filter_map(Result::err) {
        error!(logger, "Error processing Channel"; "channel" => ?channel, "error" => ?channel_err, "main" => "all_channels_tick");
    }

    info!(logger, "Processed {} channels", channels_size);

    if channels_size >= args.config.max_channels as usize {
        error!(logger, "WARNING: channel limit cfg.MAX_CHANNELS={} reached", &args.config.max_channels; "main" => "all_channels_tick");
    }
}
