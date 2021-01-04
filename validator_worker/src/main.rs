#![deny(rust_2018_idioms)]
#![deny(clippy::all)]

use std::convert::TryFrom;
use std::error::Error;
use std::time::Duration;

use clap::{crate_version, App, Arg};
use futures::future::{join, join_all};
use tokio::runtime::Runtime;
use tokio::time::{delay_for, timeout};

use adapter::{AdapterTypes, DummyAdapter, EthereumAdapter};
use primitives::adapter::{Adapter, DummyAdapterOptions, KeystoreOptions};
use primitives::config::{configuration, Config};
use primitives::util::tests::prep_db::{AUTH, IDS};
use primitives::{Channel, ChannelId, SpecValidator, ValidatorId};
use slog::{error, info, Logger};
use std::fmt::Debug;
use validator_worker::error::{Error as ValidatorWorkerError, TickError};
use validator_worker::{all_channels, follower, leader, SentryApi};

#[derive(Debug, Clone)]
struct Args<A: Adapter> {
    sentry_url: String,
    config: Config,
    adapter: A,
}

fn main() -> Result<(), Box<dyn Error>> {
    let cli = App::new("Validator worker")
        .version(crate_version!())
        .arg(
            Arg::with_name("config")
                .help("the config file for the validator worker")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("adapter")
                .long("adapter")
                .short("a")
                .help("the adapter for authentication and signing")
                .required(true)
                .default_value("ethereum")
                .possible_values(&["ethereum", "dummy"])
                .takes_value(true),
        )
        .arg(
            Arg::with_name("keystoreFile")
                .long("keystoreFile")
                .short("k")
                .help("path to the JSON Ethereum Keystore file")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("dummyIdentity")
                .long("dummyIdentity")
                .short("i")
                .help("the identity to use with the dummy adapter")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("sentryUrl")
                .long("sentryUrl")
                .short("u")
                .help("the URL to the sentry used for listing channels")
                .default_value("http://127.0.0.1:8005")
                .required(true)
                .takes_value(true),
        )
        .arg(
            Arg::with_name("singleTick")
                .long("singleTick")
                .short("t")
                .takes_value(false)
                .help("runs the validator in single-tick mode and exit"),
        )
        .get_matches();

    let environment = std::env::var("ENV").unwrap_or_else(|_| "development".into());
    let config_file = cli.value_of("config");
    let config = configuration(&environment, config_file).expect("failed to parse configuration");
    let sentry_url = cli.value_of("sentryUrl").expect("sentry url missing");
    let is_single_tick = cli.is_present("singleTick");

    let adapter = match cli.value_of("adapter").unwrap() {
        "ethereum" => {
            let keystore_file = cli
                .value_of("keystoreFile")
                .expect("unable to get keystore file");
            let keystore_pwd = std::env::var("KEYSTORE_PWD").expect("unable to get keystore pwd");
            let keystore_options = KeystoreOptions {
                keystore_file: keystore_file.to_string(),
                keystore_pwd,
            };
            AdapterTypes::EthereumAdapter(Box::new(
                EthereumAdapter::init(keystore_options, &config).expect("failed to init adapter"),
            ))
        }
        "dummy" => {
            let dummy_identity = cli
                .value_of("dummyIdentity")
                .expect("unable to get dummyIdentity");
            let options = DummyAdapterOptions {
                dummy_identity: ValidatorId::try_from(dummy_identity)?,
                dummy_auth: IDS.clone(),
                dummy_auth_tokens: AUTH.clone(),
            };
            AdapterTypes::DummyAdapter(Box::new(DummyAdapter::init(options, &config)))
        }
        // @TODO exit gracefully
        _ => panic!("We don't have any other adapters implemented yet!"),
    };

    let logger = logger();

    match adapter {
        AdapterTypes::EthereumAdapter(ethadapter) => {
            run(is_single_tick, &sentry_url, &config, *ethadapter, &logger)
        }
        AdapterTypes::DummyAdapter(dummyadapter) => {
            run(is_single_tick, &sentry_url, &config, *dummyadapter, &logger)
        }
    }
}

fn run<A: Adapter + 'static>(
    is_single_tick: bool,
    sentry_url: &str,
    config: &Config,
    mut adapter: A,
    logger: &Logger,
) -> Result<(), Box<dyn Error>> {
    // unlock adapter
    adapter.unlock()?;

    let args = Args {
        sentry_url: sentry_url.to_owned(),
        config: config.to_owned(),
        adapter,
    };

    // Create the runtime
    let mut rt = Runtime::new()?;

    if is_single_tick {
        rt.block_on(iterate_channels(args, &logger));
    } else {
        rt.block_on(infinite(args, &logger));
    }

    Ok(())
}

async fn infinite<A: Adapter + 'static>(args: Args<A>, logger: &Logger) {
    loop {
        let arg = args.clone();
        let delay_future = delay_for(Duration::from_millis(arg.config.wait_time as u64));
        let _result = join(iterate_channels(arg, logger), delay_future).await;
    }
}

async fn iterate_channels<A: Adapter + 'static>(args: Args<A>, logger: &Logger) {
    let result = all_channels(&args.sentry_url, args.adapter.whoami()).await;

    let channels = match result {
        Ok(channels) => channels,
        Err(e) => {
            error!(logger, "Failed to get channels"; "error" => ?e, "main" => "iterate_channels");
            return;
        }
    };

    let channels_size = channels.len();

    let tick_results = join_all(
        channels
            .into_iter()
            .map(|channel| validator_tick(args.adapter.clone(), channel, &args.config, logger)),
    )
    .await;

    for channel_err in tick_results.into_iter().filter_map(Result::err) {
        error!(logger, "Error processing channel"; "channel_error" => ?channel_err, "main" => "iterate_channels");
    }

    info!(logger, "Processed {} channels", channels_size);

    if channels_size >= args.config.max_channels as usize {
        error!(logger, "WARNING: channel limit cfg.MAX_CHANNELS={} reached", &args.config.max_channels; "main" => "iterate_channels");
    }
}

async fn validator_tick<A: Adapter + 'static>(
    adapter: A,
    channel: Channel,
    config: &Config,
    logger: &Logger,
) -> Result<(ChannelId, Box<dyn Debug>), ValidatorWorkerError<A::AdapterError>> {
    let whoami = *adapter.whoami();

    // Cloning the `Logger` is cheap, see documentation for more info
    let sentry = SentryApi::init(adapter, channel.clone(), &config, logger.clone())
        .map_err(ValidatorWorkerError::SentryApi)?;
    let duration = Duration::from_millis(config.validator_tick_timeout as u64);

    match channel.spec.validators.find(&whoami) {
        Some(SpecValidator::Leader(_)) => match timeout(duration, leader::tick(&sentry)).await {
            Err(timeout_e) => Err(ValidatorWorkerError::LeaderTick(
                channel.id,
                TickError::TimedOut(timeout_e),
            )),
            Ok(Err(tick_e)) => Err(ValidatorWorkerError::LeaderTick(
                channel.id,
                TickError::Tick(tick_e),
            )),
            Ok(Ok(tick_status)) => {
                info!(&logger, "Leader tick"; "status" => ?tick_status);
                Ok((channel.id, Box::new(tick_status)))
            }
        },
        Some(SpecValidator::Follower(_)) => {
            match timeout(duration, follower::tick(&sentry)).await {
                Err(timeout_e) => Err(ValidatorWorkerError::FollowerTick(
                    channel.id,
                    TickError::TimedOut(timeout_e),
                )),
                Ok(Err(tick_e)) => Err(ValidatorWorkerError::FollowerTick(
                    channel.id,
                    TickError::Tick(tick_e),
                )),
                Ok(Ok(tick_status)) => {
                    info!(&logger, "Follower tick"; "status" => ?tick_status);
                    Ok((channel.id, Box::new(tick_status)))
                }
            }
        }
        // @TODO: Can we make this so that we don't have this check at all? maybe something with the SentryApi struct?
        None => unreachable!("SentryApi makes a check if validator is in Channel spec on `init()`"),
    }
}

fn logger() -> Logger {
    use primitives::util::logging::{Async, PrefixedCompactFormat, TermDecorator};
    use slog::{o, Drain};

    let decorator = TermDecorator::new().build();
    let drain = PrefixedCompactFormat::new("validator_worker", decorator).fuse();
    let drain = Async::new(drain).build().fuse();

    Logger::root(drain, o!())
}
