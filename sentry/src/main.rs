#![deny(clippy::all)]
#![deny(rust_2018_idioms)]

use adapter::{primitives::AdapterTypes, Adapter};
use clap::{crate_version, Arg, Command};

use primitives::{
    config::configuration, postgres::POSTGRES_CONFIG, test_util::DUMMY_AUTH,
    util::logging::new_logger, ValidatorId,
};
use sentry::{
    db::{postgres_connection, redis_connection, setup_migrations, CampaignRemaining},
    platform::PlatformApi,
    Application,
};
use slog::info;
use std::{env, net::SocketAddr};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Command::new("Sentry")
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
                .possible_values(&["ethereum", "dummy"])
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

            let options = adapter::ethereum::Options {
                keystore_file: keystore_file.to_string(),
                keystore_pwd,
            };
            let ethereum_adapter = Adapter::new(
                adapter::Ethereum::init(options, &config)
                    .expect("Should initialize ethereum adapter"),
            );

            AdapterTypes::Ethereum(Box::new(ethereum_adapter))
        }
        "dummy" => {
            let dummy_identity = cli
                .value_of("dummyIdentity")
                .expect("Dummy identity is required for the dummy adapter");

            let options = adapter::dummy::Options {
                dummy_identity: ValidatorId::try_from(dummy_identity)
                    .expect("failed to parse dummy identity"),
                dummy_auth_tokens: DUMMY_AUTH.clone(),
            };

            let dummy_adapter = Adapter::new(adapter::Dummy::init(options));
            AdapterTypes::Dummy(Box::new(dummy_adapter))
        }
        _ => panic!("You can only use `ethereum` & `dummy` adapters!"),
    };

    let logger = new_logger("sentry");
    let redis = redis_connection(env_config.redis_url).await?;
    info!(&logger, "Checking connection and applying migrations...");
    // Check connection and setup migrations before setting up Postgres
    setup_migrations(env_config.env).await;

    // use the environmental variables to setup the Postgres connection
    let postgres = match postgres_connection(42, POSTGRES_CONFIG.clone()).await {
        Ok(pool) => pool,
        Err(build_err) => panic!("Failed to build postgres database pool: {build_err}"),
    };

    let campaign_remaining = CampaignRemaining::new(redis.clone());

    // todo: Make platform_url configurable! Load from config or pass with env. variable
    let platform_url = "https://platform.adex.network"
        .parse()
        .expect("Bad ApiUrl, load from Config?");
    // todo: Make keep_alive_interval configurable!
    let platform_api = PlatformApi::new(
        platform_url,
        std::time::Duration::from_secs(3),
        logger.clone(),
    )
    .expect("Should make PlatformApi");

    match adapter {
        AdapterTypes::Ethereum(adapter) => {
            Application::new(
                *adapter,
                config,
                logger,
                redis,
                postgres,
                campaign_remaining,
                platform_api,
            )
            .run(socket_addr)
            .await
        }
        AdapterTypes::Dummy(adapter) => {
            Application::new(
                *adapter,
                config,
                logger,
                redis,
                postgres,
                campaign_remaining,
                platform_api,
            )
            .run(socket_addr)
            .await
        }
    };

    Ok(())
}
