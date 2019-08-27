#![feature(async_await, await_macro)]
#![deny(rust_2018_idioms)]
#![deny(clippy::all)]

use adapter::{AdapterTypes, DummyAdapter, EthereumAdapter};
use clap::{App, Arg};
use primitives::adapter::{Adapter, AdapterOptions};
use primitives::config::configuration;

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
                .short("s")
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
    let config_file = cli.value_of("config").unwrap_or("");
    let config = configuration(&environment, Some(&config_file)).unwrap();
    let _sentry_url = cli.value_of("sentryUrl").unwrap();
    let is_single_tick = cli.is_present("singleTick");

    let adapter = match cli.value_of("adapter").unwrap() {
        "ethereum" => {
            let keystore_file = cli.value_of("keystoreFile").unwrap();
            let keystore_pwd = std::env::var("KEYSTORE_PWD").unwrap();

            let options = AdapterOptions {
                keystore_file: Some(keystore_file.to_string()),
                keystore_pwd: Some(keystore_pwd),
                dummy_identity: None,
                dummy_auth: None,
                dummy_auth_tokens: None,
            };
            AdapterTypes::EthereumAdapter(EthereumAdapter::init(options, &config))
        }
        "dummy" => {
            let dummy_identity = cli.value_of("dummyIdentity").unwrap();
            let options = AdapterOptions {
                dummy_identity: Some(dummy_identity.to_string()),
                // this should be prefilled using fixtures
                //
                dummy_auth: None,
                dummy_auth_tokens: None,
                keystore_file: None,
                keystore_pwd: None,
            };
            AdapterTypes::DummyAdapter(DummyAdapter::init(options, &config))
        }
        // @TODO exit gracefully
        _ => panic!("We don't have any other adapters implemented yet!"),
    };

    match adapter {
        AdapterTypes::EthereumAdapter(ethadapter) => run(is_single_tick, ethadapter),
        AdapterTypes::DummyAdapter(dummyadapter) => run(is_single_tick, dummyadapter),
    }
}

fn run(_is_single_tick: bool, _adapter: impl Adapter) {
    // let sentry = SentryApi {
    //     client: Client::new(),
    //     sentry_url: CONFIG.sentry_url.clone(),
    // };
    //    if !is_single_tick {
    //     let worker = InfiniteWorker {
    //         tick_worker,
    //         ticks_wait_time: CONFIG.ticks_wait_time,
    //     };

    //     tokio::run(
    //         async move {
    //             await!(worker.run()).unwrap();
    //         }
    //             .unit_error()
    //             .boxed()
    //             .compat(),
    //     );
    // } else {
    //     tokio::run(
    //         async move {
    //             await!(tick_worker.run()).unwrap();
    //         }
    //             .unit_error()
    //             .boxed()
    //             .compat(),
    //     );
    // }
}
