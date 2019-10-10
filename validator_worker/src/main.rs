#![deny(rust_2018_idioms)]
#![deny(clippy::all)]

use clap::{App, Arg};

use adapter::{AdapterTypes, DummyAdapter, EthereumAdapter};
use primitives::adapter::{Adapter, AdapterOptions, KeystoreOptions};
use primitives::config::{configuration, Config};
use primitives::util::tests::prep_db::{AUTH, IDS};

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

// @TODO work in separate pull request
fn run(_is_single_tick: bool, sentry: &str, config: &Config, adapter: impl Adapter + 'static) {
    let _sentry_url = sentry.to_owned();
    let _adapter = adapter.clone();
    let _config = config.clone();

    // let result = async move {
    //     let channels = await!(all_channels(&sentry_url, adapter.clone())).unwrap();
    //     println!("{:?}", channels);
    //     for channel in channels.into_iter() {
    //         let sentry = SentryApi::new(adapter.clone(), &channel, &config, true);
    //         let whoami = adapter.whoami();
    //         let index = channel.spec.validators.into_iter().position(|v| v.id == whoami);
    //         let tick = match index {
    //             Some(0) => Leader.tick(&channel),
    //             Some(1) => Follower.tick(&channel)
    //         };
    //     }
    //     Ok(())
    // };
    // @TODO hanlde errors more gracefully
    // tokio::run(result.map_err(|e: Box<dyn Error>| panic!("{}", e)).boxed().compat())
}
