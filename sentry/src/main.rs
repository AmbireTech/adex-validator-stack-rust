#![deny(clippy::all)]
#![deny(rust_2018_idioms)]

use clap::{App, Arg};

use adapter::{AdapterTypes, DummyAdapter, EthereumAdapter};
use primitives::adapter::{DummyAdapterOptions, KeystoreOptions, Adapter};
use primitives::config::configuration;
use primitives::util::logging::{Async, PrefixedCompactFormat, TermDecorator};
use primitives::util::tests::prep_db::{AUTH, IDS};
use primitives::ValidatorId;
use sentry::db::{postgres_connection, redis_connection};
use sentry::Application;
use slog::{o, Drain, info, error, Logger};
use std::convert::TryFrom;
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Error, Method, Request, Response, Server, StatusCode};

const DEFAULT_PORT: u16 = 8005;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = App::new("Sentry")
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
            Arg::with_name("clustered")
                .short("c")
                .help("Run app in cluster mode with multiple workers"),
        )
        .get_matches();

    let environment = std::env::var("ENV").unwrap_or_else(|_| "development".into());
    let port = std::env::var("PORT")
        .map(|s| s.parse::<u16>().expect("Invalid port(u16) was provided"))
        .unwrap_or_else(|_| DEFAULT_PORT);
    let config_file = cli.value_of("config");
    let config = configuration(&environment, config_file).unwrap();
    let clustered = cli.is_present("clustered");

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
                dummy_auth: IDS.clone(),
                dummy_auth_tokens: AUTH.clone(),
            };

            let dummy_adapter = DummyAdapter::init(options, &config);
            AdapterTypes::DummyAdapter(Box::new(dummy_adapter))
        }
        // @TODO exit gracefully
        _ => panic!("We don't have any other adapters implemented yet!"),
    };

    let logger = logger();
    let redis = redis_connection().await?;
    let postgres = postgres_connection().await?;

    match adapter {
        AdapterTypes::EthereumAdapter(adapter) => run(Application::new(*adapter, config, logger, redis, postgres, clustered, port)).await,
        AdapterTypes::DummyAdapter(adapter) =>  run(Application::new(*adapter, config, logger, redis, postgres, clustered, port)).await
    };

    Ok(())
}

/// Starts the `hyper` `Server`.
async fn run<A: Adapter + 'static>(app: Application<A>) {
    let addr = ([127, 0, 0, 1], app.port).into();
    info!(&app.logger, "Listening on port {}!", app.port);

    let make_service = make_service_fn(move |_| {
        let server = app.clone();
        async move {
            Ok::<_, Error>(service_fn(move |req| {
                let server = server.clone();
                async move {
                    Ok::<_, Error>(
                        server.handle_routing(
                            req
                        )
                        .await,
                    )
                }
            }))
        }
    });

    let server = Server::bind(&addr).serve(make_service);

    if let Err(e) = server.await {
        // error!(&self.logger, "server error: {}", e);
    }
}

fn logger() -> Logger {
    let decorator = TermDecorator::new().build();
    let drain = PrefixedCompactFormat::new("sentry", decorator).fuse();
    let drain = Async::new(drain).build().fuse();

    Logger::root(drain, o!())
}
