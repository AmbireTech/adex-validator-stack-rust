#![deny(rust_2018_idioms)]
#![deny(clippy::all)]

use clap::{App, Arg};

use adapter::{AdapterTypes, DummyAdapter, EthereumAdapter};
use futures::compat::Future01CompatExt;
use futures::future::try_join_all;
use futures::future::{join, FutureExt, TryFutureExt};
use futures::lock::Mutex;
use primitives::adapter::{Adapter, AdapterOptions, KeystoreOptions};
use primitives::config::{configuration, Config};
use primitives::util::tests::prep_db::{AUTH, IDS};
use primitives::Channel;
use std::ops::Add;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::timer::Delay;
use tokio::util::FutureExt as TokioFutureExt;
use validator_worker::{all_channels, follower, leader, SentryApi};

fn main() {
    let cli = App::new("Validator worker")
        .version("0.1")
        .arg(
            Arg::with_name("config")
                .help("the config file for the validator worker")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("adapter")
                .short("a")
                .help("the adapter for authentication and signing")
                .required(true)
                .default_value("ethereum")
                .possible_values(&["ethereum", "dummy"])
                .takes_value(true),
        )
        .arg(
            Arg::with_name("keystoreFile")
                .short("k")
                .help("path to the JSON Ethereum Keystore file")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("dummyIdentity")
                .short("i")
                .help("the identity to use with the dummy adapter")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("sentryUrl")
                .short("u")
                .help("the URL to the sentry used for listing channels")
                .default_value("http://127.0.0.1:8005")
                .required(true)
                .takes_value(true),
        )
        .arg(
            Arg::with_name("singleTick")
                .short("s")
                .help("Runs the validator in single-tick mode and exits"),
        )
        .get_matches();

    let environment = std::env::var("ENV").unwrap_or_else(|_| "development".into());
    let config_file = cli.value_of("config");
    let config = configuration(&environment, config_file).unwrap();
    let sentry_url = cli.value_of("sentryUrl").unwrap();
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
            let options = AdapterOptions::EthereumAdapter(keystore_options);
            AdapterTypes::EthereumAdapter(Box::new(
                EthereumAdapter::init(options, &config).expect("failed to init adapter"),
            ))
        }
        "dummy" => {
            let dummy_identity = cli.value_of("dummyIdentity").unwrap();
            let options = AdapterOptions::DummAdapter {
                dummy_identity: dummy_identity.to_string(),
                dummy_auth: IDS.clone(),
                dummy_auth_tokens: AUTH.clone(),
            };
            AdapterTypes::DummyAdapter(Box::new(
                DummyAdapter::init(options, &config).expect("failed to init adapter"),
            ))
        }
        // @TODO exit gracefully
        _ => panic!("We don't have any other adapters implemented yet!"),
    };

    match adapter {
        AdapterTypes::EthereumAdapter(ethadapter) => {
            run(is_single_tick, &sentry_url, &config, *ethadapter)
        }
        AdapterTypes::DummyAdapter(dummyadapter) => {
            run(is_single_tick, &sentry_url, &config, *dummyadapter)
        }
    }
}

fn run<A: Adapter + 'static>(is_single_tick: bool, sentry_url: &str, config: &Config, adapter: A) {
    let _config = config.clone();
    let sentry_adapter = Arc::new(Mutex::new(adapter.clone()));
    let whoami = adapter.whoami();

    if is_single_tick {
        tokio::run(
            iterate_channels(
                sentry_url.to_owned(),
                config.clone(),
                sentry_adapter.clone(),
                whoami.clone(),
            )
            .boxed()
            .compat(),
        );
    } else {
        tokio::run(
            infinite(
                sentry_url.to_owned(),
                config.clone(),
                sentry_adapter.clone(),
                whoami.clone(),
            )
            .boxed()
            .compat(),
        );
    }
}

async fn infinite<A: Adapter + 'static>(
    sentry_url: String,
    config: Config,
    adapter: Arc<Mutex<A>>,
    whoami: String,
) -> Result<(), ()> {
    loop {
        let delay_future =
            Delay::new(Instant::now().add(Duration::from_secs(config.wait_time as u64)));
        let joined = join(
            iterate_channels(
                sentry_url.clone(),
                config.clone(),
                adapter.clone(),
                whoami.clone(),
            ),
            delay_future.compat(),
        );
        joined.await;
    }
}

async fn iterate_channels<A: Adapter + 'static>(
    sentry_url: String,
    config: Config,
    adapter: Arc<Mutex<A>>,
    whoami: String,
) -> Result<(), ()> {
    let channels = all_channels(&sentry_url, whoami.to_owned())
        .await
        .expect("Failed to get channels");

    println!("{:?}", channels);
    let channels_size = channels.len();

    try_join_all(
        channels
            .into_iter()
            .map(|channel| validator_tick(adapter.clone(), channel, &config, &whoami)),
    )
    .await
    .expect("Failed to iterate channels");

    println!("processed {} channels", channels_size);
    if channels_size >= config.max_channels as usize {
        println!(
            "WARNING: channel limit cfg.MAX_CHANNELS={} reached",
            config.max_channels
        )
    }

    Ok(())
}

async fn validator_tick<A: Adapter + 'static>(
    adapter: Arc<Mutex<A>>,
    channel: Channel,
    config: &Config,
    whoami: &str,
) -> Result<(), ()> {
    let sentry =
        SentryApi::new(adapter, &channel, &config, true, whoami).expect("Failed to init sentry");
    let index = channel
        .spec
        .validators
        .into_iter()
        .position(|v| v.id == *whoami);
    match index {
        Some(0) => {
            if let Err(e) = leader::tick(&sentry)
                .boxed()
                .compat()
                .timeout(Duration::from_secs(config.validator_tick_timeout as u64))
                .compat()
                .await
            {
                eprintln!("{}", e);
            }
        }
        Some(1) => {
            if let Err(e) = follower::tick(&sentry)
                .boxed()
                .compat()
                .timeout(Duration::from_secs(config.validator_tick_timeout as u64))
                .compat()
                .await
            {
                eprintln!("{}", e);
            }
        }
        Some(_) => eprintln!("validatorTick: processing a channel where we are not validating"),
        None => eprintln!("validatorTick: processing a channel where we are not validating"),
    };
    Ok(())
}
