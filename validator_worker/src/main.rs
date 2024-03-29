#![deny(rust_2018_idioms)]
#![deny(clippy::all)]

use std::{env::VarError, error::Error};

use clap::{crate_version, Arg, Command};

use adapter::{primitives::AdapterTypes, Adapter, Dummy, Ethereum};
use primitives::{
    config::{configuration, Environment},
    test_util::DUMMY_AUTH,
    util::logging::new_logger,
    ValidatorId,
};
use validator_worker::{SentryApi, Worker};

fn main() -> Result<(), Box<dyn Error>> {
    let cli = Command::new("Validator worker")
        .version(crate_version!())
        .arg(
            Arg::new("config")
                .help("the config file for the validator worker")
                .takes_value(true),
        )
        .arg(
            Arg::new("adapter")
                .long("adapter")
                .short('a')
                .help("the adapter for authentication and signing")
                .required(true)
                .default_value("ethereum")
                .possible_values(["ethereum", "dummy"])
                .takes_value(true),
        )
        .arg(
            Arg::new("keystoreFile")
                .long("keystoreFile")
                .short('k')
                .help("path to the JSON Ethereum Keystore file")
                .takes_value(true),
        )
        .arg(
            Arg::new("dummyIdentity")
                .long("dummyIdentity")
                .short('i')
                .help("the identity to use with the dummy adapter")
                .takes_value(true),
        )
        .arg(
            Arg::new("sentryUrl")
                .long("sentryUrl")
                .short('u')
                .help("the URL to the sentry used for listing channels")
                .default_value("http://127.0.0.1:8005")
                .required(true)
                .takes_value(true),
        )
        .arg(
            Arg::new("singleTick")
                .long("singleTick")
                .short('t')
                .takes_value(false)
                .help("runs the validator in single-tick mode and exit"),
        )
        .get_matches();

    let environment: Environment = match std::env::var("ENV") {
        Ok(string) => serde_json::from_value(serde_json::Value::String(string))
            .expect("Valid Environment - development or production"),
        Err(VarError::NotPresent) => Environment::default(),
        Err(err) => panic!("Invalid `ENV`: {err}"),
    };

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
                dummy_auth_tokens: DUMMY_AUTH.clone(),
                dummy_chains: config.chains.values().cloned().collect(),
            };
            let adapter = Adapter::with_unlocked(Dummy::init(options));

            AdapterTypes::dummy(adapter)
        }
        // @TODO exit gracefully
        _ => panic!("We don't have any other adapters implemented yet!"),
    };

    let logger = new_logger("validator_worker");

    match unlocked_adapter {
        AdapterTypes::Ethereum(eth_adapter) => {
            let sentry = SentryApi::new(*eth_adapter, logger.clone(), config, sentry_url)
                .expect("Should create the SentryApi");

            Worker::from_sentry(sentry).run(is_single_tick)
        }
        AdapterTypes::Dummy(dummy_adapter) => {
            let sentry = SentryApi::new(*dummy_adapter, logger.clone(), config, sentry_url)
                .expect("Should create the SentryApi");

            Worker::from_sentry(sentry).run(is_single_tick)
        }
    }
}
