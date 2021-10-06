#![deny(rust_2018_idioms)]
#![deny(clippy::all)]

use std::{convert::TryFrom, error::Error, time::Duration};

use clap::{crate_version, App, Arg};
use futures::{
    future::{join, join_all},
    TryFutureExt,
};
use tokio::{runtime::Runtime, time::sleep};

use adapter::{AdapterTypes, DummyAdapter, EthereumAdapter};
use primitives::{
    adapter::{Adapter, DummyAdapterOptions, KeystoreOptions},
    config::{configuration, Config},
    util::{
        tests::prep_db::{AUTH, IDS},
        ApiUrl,
    },
    ValidatorId,
};
use slog::{error, info, Logger};
use std::fmt::Debug;
use validator_worker::{
    channel::{channel_tick, collect_channels},
    SentryApi,
};

#[derive(Debug, Clone)]
struct Args<A: Adapter> {
    sentry_url: ApiUrl,
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
    let sentry_url = cli
        .value_of("sentryUrl")
        .expect("sentry url missing")
        .parse()?;
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
            run(is_single_tick, sentry_url, &config, *ethadapter, &logger)
        }
        AdapterTypes::DummyAdapter(dummyadapter) => {
            run(is_single_tick, sentry_url, &config, *dummyadapter, &logger)
        }
    }
}

fn run<A: Adapter + 'static>(
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

async fn infinite<A: Adapter + 'static>(args: Args<A>, logger: &Logger) {
    loop {
        let arg = args.clone();
        let wait_time_future = sleep(Duration::from_millis(arg.config.wait_time as u64));

        let _result = join(all_channels_tick(arg, logger), wait_time_future).await;
    }
}

async fn all_channels_tick<A: Adapter + 'static>(args: Args<A>, logger: &Logger) {
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

fn logger() -> Logger {
    use primitives::util::logging::{Async, PrefixedCompactFormat, TermDecorator};
    use slog::{o, Drain};

    let decorator = TermDecorator::new().build();
    let drain = PrefixedCompactFormat::new("validator_worker", decorator).fuse();
    let drain = Async::new(drain).build().fuse();

    Logger::root(drain, o!())
}
