#![deny(clippy::all)]
#![deny(rust_2018_idioms)]

use std::{env, net::SocketAddr, path::PathBuf};

use clap::{crate_version, value_parser, Arg, Command};

use redis::aio::MultiplexedConnection;
use slog::info;

use adapter::{primitives::AdapterTypes, Adapter};
use primitives::{
    config::{configuration, Environment},
    postgres::POSTGRES_CONFIG,
    test_util::DUMMY_AUTH,
    util::logging::new_logger,
    ValidatorId,
};
use sentry::{
    application::{
        seed::{seed_dummy, seed_ethereum},
        EnableTls, EnvConfig,
    },
    db::{postgres_connection, redis_connection, setup_migrations, CampaignRemaining, DbPool},
    platform::PlatformApi,
    Application,
};

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
            Arg::new("certificates")
                .long("certificates")
                .help("Certificates .pem file for TLS")
                .value_parser(value_parser!(PathBuf))
                .takes_value(true),
        )
        .arg(
            Arg::new("privateKeys")
                .long("privateKeys")
                .help("The Private keys .pem file for TLS (PKCS8)")
                .value_parser(value_parser!(PathBuf))
                .takes_value(true),
        )
        .get_matches();

    let env_config = sentry::application::EnvConfig::from_env()?;

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
                dummy_chains: config.chains.values().cloned().collect(),
            };

            let dummy_adapter = Adapter::new(adapter::Dummy::init(options));
            AdapterTypes::Dummy(Box::new(dummy_adapter))
        }
        _ => panic!("You can only use `ethereum` & `dummy` adapters!"),
    };

    let enable_tls = match (
        cli.get_one::<PathBuf>("certificates"),
        cli.get_one::<PathBuf>("privateKeys"),
    ) {
        (Some(certs_path), Some(private_keys)) => {
            EnableTls::new_tls(certs_path, private_keys, socket_addr)
                .await
                .expect("Failed to load certificates & private key files")
        }
        (None, None) => EnableTls::no_tls(socket_addr),
        _ => panic!(
            "You should pass both --certificates & --privateKeys options to enable TLS or neither"
        ),
    };

    let logger = new_logger("sentry");

    let (redis, postgres) = setup_databases(&logger, &env_config).await?;

    let campaign_remaining = CampaignRemaining::new(redis.clone());

    let platform_api = PlatformApi::new(
        config.sentry.platform.url.clone(),
        config.sentry.platform.keep_alive_interval,
    )
    .expect("Failed to build PlatformApi");

    // Like dummy adapter route
    match adapter {
        AdapterTypes::Ethereum(adapter) => {
            let app = Application::new(
                *adapter,
                config.clone(),
                logger,
                redis.clone(),
                postgres,
                campaign_remaining,
                platform_api,
            );

            if env_config.seed_db && Environment::Development == env_config.env {
                seed_ethereum(app.clone()).await?;
            }

            app.run(enable_tls).await
        }
        AdapterTypes::Dummy(adapter) => {
            let app = Application::new(
                *adapter,
                config.clone(),
                logger,
                redis.clone(),
                postgres,
                campaign_remaining,
                platform_api,
            );

            if env_config.seed_db && Environment::Development == env_config.env {
                seed_dummy(app.clone()).await?;
            }

            app.run(enable_tls).await
        }
    };

    Ok(())
}

/// Setup the databases before use in the application:
///
/// 1. Runs migrations on `postgres` but if [`Environment::Development`] then it runs them down first.
/// 2. Flushes `redis` if [`Environment::Development`].
async fn setup_databases(
    logger: &slog::Logger,
    env_config: &EnvConfig,
) -> Result<(MultiplexedConnection, DbPool), Box<dyn std::error::Error>> {
    let redis = redis_connection(env_config.redis_url.clone()).await?;

    info!(&logger, "Checking connection and applying migrations...");
    // Check connection and setup migrations before setting up Postgres
    tokio::task::block_in_place(|| {
        // Migrations are blocking, so we need to wrap it with block_in_place
        // otherwise we get a tokio error
        setup_migrations(env_config.env)
    });

    // clearing up redis
    if Environment::Development == env_config.env {
        info!(&logger, "Flushing redis...");
        redis::cmd("FLUSHDB")
            .query_async::<_, String>(&mut redis.clone())
            .await?;
    }

    // use the environmental variables to setup the Postgres connection
    let postgres = match postgres_connection(POSTGRES_CONFIG.clone()).await {
        Ok(pool) => pool,
        Err(build_err) => panic!("Failed to build postgres database pool: {build_err}"),
    };

    Ok((redis, postgres))
}
