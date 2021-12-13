#![deny(rust_2018_idioms)]
#![deny(clippy::all)]

use std::error::Error;

use clap::{crate_version, App, Arg};

use adapter::{prelude::*, primitives::AdapterTypes, Adapter, Dummy, Ethereum};
use primitives::{
    config::{configuration, Environment},
    util::{
        logging::new_logger,
        tests::prep_db::{ADDRESSES, AUTH},
    },
    ValidatorId,
};
use validator_worker::{sentry_interface::Validator, SentryApi, Worker};

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

    let environment: Environment = serde_json::from_value(serde_json::Value::String(
        std::env::var("ENV").expect("Valid environment variable"),
    ))
    .expect("Valid Environment - development or production");

    let config_file = cli.value_of("config");
    let config = configuration(environment, config_file).expect("failed to parse configuration");
    let sentry_url = cli
        .value_of("sentryUrl")
        .expect("sentry url missing")
        .parse()?;
    let is_single_tick = cli.is_present("singleTick");

    let unlocked_adapter = match cli.value_of("adapter").unwrap() {
        "ethereum" => {
            let keystore_file = cli
                .value_of("keystoreFile")
                .expect("unable to get keystore file");
            let keystore_pwd = std::env::var("KEYSTORE_PWD").expect("unable to get keystore pwd");
            let keystore_options = adapter::ethereum::Options {
                keystore_file: keystore_file.to_string(),
                keystore_pwd,
            };

            let ethereum =
                Ethereum::init(keystore_options, &config).expect("failed to init Ethereum adapter");
            // .unlock()
            // .expect("failed to Unlock Ethereum adapter");
            let adapter = Adapter::new(ethereum)
                .unlock()
                .expect("failed to Unlock Ethereum adapter");

            AdapterTypes::ethereum(adapter)
        }
        "dummy" => {
            let dummy_identity = cli
                .value_of("dummyIdentity")
                .expect("unable to get dummyIdentity");
            let options = adapter::dummy::Options {
                dummy_identity: ValidatorId::try_from(dummy_identity)?,
                dummy_auth: ADDRESSES.clone(),
                dummy_auth_tokens: AUTH.clone(),
            };
            let adapter = Adapter::with_unlocked(Dummy::init(options, &config));

            AdapterTypes::dummy(adapter)
        }
        // @TODO exit gracefully
        _ => panic!("We don't have any other adapters implemented yet!"),
    };

    let logger = new_logger("validator_worker");

    match unlocked_adapter {
        AdapterTypes::Ethereum(eth_adapter) => {
            let whoami = Validator {
                url: sentry_url,
                token: eth_adapter
                    .get_auth(eth_adapter.whoami())
                    .expect("Failed to get Authentication token for Who am I"),
            };

            let sentry = SentryApi::new(*eth_adapter, logger.clone(), config, whoami)
                .expect("Should create the SentryApi");

            Worker::from_sentry(sentry).run(is_single_tick)
        }
        AdapterTypes::Dummy(dummy_adapter) => {
            let whoami = Validator {
                url: sentry_url,
                token: dummy_adapter
                    .get_auth(dummy_adapter.whoami())
                    .expect("Failed to get Authentication token for Who am I"),
            };

            let sentry = SentryApi::new(*dummy_adapter, logger.clone(), config, whoami)
                .expect("Should create the SentryApi");

            Worker::from_sentry(sentry).run(is_single_tick)
        }
    }
}
