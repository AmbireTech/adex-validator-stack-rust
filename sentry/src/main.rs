#![deny(clippy::all)]
#![deny(rust_2018_idioms)]

use clap::{crate_version, App, Arg};

use adapter::{AdapterTypes, DummyAdapter, EthereumAdapter};
use primitives::{
    adapter::{DummyAdapterOptions, KeystoreOptions},
    config::configuration,
    postgres::POSTGRES_CONFIG,
    test_util::ADDRESSES,
    util::{logging::new_logger, tests::prep_db::AUTH},
    ValidatorId,
};
use sentry::{
    db::{postgres_connection, redis_connection, setup_migrations, CampaignRemaining},
    Application,
};
use slog::info;
use std::{env, net::SocketAddr};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = App::new("Sentry")
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
        .get_matches();

    let env_config = sentry::application::Config::from_env()?;

    let socket_addr: SocketAddr = (env_config.ip_addr, env_config.port).into();

    let config_file = cli.value_of("config");
    let config = configuration(env_config.env, config_file).unwrap();

    let adapter = match cli.value_of("adapter").unwrap() {
        "ethereum" => {
            let keystore_file = cli
                .value_of("keystoreFile")
                .expect("keystore file is required for the ethereum adapter");
            let keystore_pwd = std::env::var("KEYSTORE_PWD").expect("unable to get keystore pwd");

            let options = KeystoreOptions {
                keystore_file: keystore_file.to_string(),
                keystore_pwd,
            };
            let ethereum_adapter = EthereumAdapter::init(options, &config)
                .expect("Should initialize ethereum adapter");

            AdapterTypes::EthereumAdapter(Box::new(ethereum_adapter))
        }
        "dummy" => {
            let dummy_identity = cli
                .value_of("dummyIdentity")
                .expect("Dummy identity is required for the dummy adapter");

            let options = DummyAdapterOptions {
                dummy_identity: ValidatorId::try_from(dummy_identity)
                    .expect("failed to parse dummy identity"),
                dummy_auth: ADDRESSES.clone(),
                dummy_auth_tokens: AUTH.clone(),
            };

            let dummy_adapter = DummyAdapter::init(options, &config);
            AdapterTypes::DummyAdapter(Box::new(dummy_adapter))
        }
        _ => panic!("You can only use `ethereum` & `dummy` adapters!"),
    };

    let logger = new_logger("sentry");
    let redis = redis_connection(env_config.redis_url).await?;
    info!(&logger, "Checking connection and applying migrations...");
    // Check connection and setup migrations before setting up Postgres
    setup_migrations(env_config.env).await;

    // use the environmental variables to setup the Postgres connection
    let postgres = postgres_connection(42, POSTGRES_CONFIG.clone()).await;
    let campaign_remaining = CampaignRemaining::new(redis.clone());

    match adapter {
        AdapterTypes::EthereumAdapter(adapter) => {
            Application::new(
                *adapter,
                config,
                logger,
                redis,
                postgres,
                campaign_remaining,
            )
            .run(socket_addr)
            .await
        }
        AdapterTypes::DummyAdapter(adapter) => {
            Application::new(
                *adapter,
                config,
                logger,
                redis,
                postgres,
                campaign_remaining,
            )
            .run(socket_addr)
            .await
        }
    };

    Ok(())
}
