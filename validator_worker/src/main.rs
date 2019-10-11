#![deny(rust_2018_idioms)]
#![deny(clippy::all)]

use clap::{App, Arg};

use adapter::{AdapterTypes, DummyAdapter, EthereumAdapter};
use futures::compat::Future01CompatExt;
use futures::future::try_join_all;
use futures::future::{join, FutureExt, TryFutureExt};
use std::ops::Add;
use std::time::{Duration, Instant};
use tokio::timer::Delay;
use tokio::util::FutureExt as TokioFutureExt;
use futures::lock::Mutex;

use primitives::adapter::{Adapter, AdapterOptions, KeystoreOptions};
use primitives::config::{configuration, Config};
use primitives::util::tests::prep_db::{AUTH, IDS};
use primitives::{Channel};
use std::error::Error;
use std::sync::{Arc, RwLock};
use validator_worker::{all_channels, follower, leader, SentryApi};

const VALIDATOR_TICK_TIMEOUT: u64 = 5000;

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
    let _adapter = adapter.clone();
    let _config = config.clone();

    if is_single_tick {
        tokio::run(iterate_channels(sentry_url, config, _adapter).boxed())
    } else {
        tokio::run(infinite(sentry_url, config, _adapter).boxed())
    }
}

async fn infinite<A: Adapter + 'static>(
    sentry_url: &str,
    config: &Config,
    adapter: A,
) -> Result<(), ()> {
    loop {
        let delay_future = Delay::new(Instant::now().add(config.wait_time));
        let joined = join(
            iterate_channels(sentry_url, config, _adapter),
            delay_future.compat(),
        );
        joined.await;
    }
}

async fn iterate_channels<A: Adapter + 'static>(
    sentry_url: &str,
    config: &Config,
    adapter: A,
) -> Result<(), Box<dyn Error>> {
    let whoami = adapter.whoami();
    let channels = all_channels(&sentry_url, whoami.clone())
        .await
        .expect("Failed to get channels");

    println!("{:?}", channels);

    let sentry_adapter = Arc::new(Mutex::new(adapter));

    let mut all = try_join_all(
        channels
            .into_iter()
            .map(|channel| validator_tick(sentry_adapter.clone(), &channel, config, &whoami)),
    )
    .await
    .unwrap();

    Ok(())
}

async fn validator_tick<A: Adapter + 'static>(
    adapter: Arc<Mutex<A>>,
    channel: &Channel,
    config: &Config,
    whoami: &str
) -> Result<(), ()> {
    let sentry = SentryApi::new(adapter, &channel, &config, true, whoami)
        .expect("Failed to init sentry");
    let index = channel
        .spec
        .validators
        .into_iter()
        .position(|v| v.id == *whoami);
    let result = match index {
        Some(0) => {
            let result = leader::tick(&sentry)
                .boxed()
                .compat()
                .timeout(Duration::from_secs(5))
                .compat()
                .await;
        }
        Some(1) => {
            let result = follower::tick(&sentry)
                .boxed()
                .compat()
                .timeout(Duration::from_secs(5))
                .compat()
                .await;
        }
    };
    Ok(())
}
