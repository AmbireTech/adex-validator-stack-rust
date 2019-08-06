#![feature(async_await, await_macro)]
#![deny(rust_2018_idioms)]
#![deny(clippy::all)]

use std::time::Duration;

use adapter::Adapter;
use lazy_static::lazy_static;

lazy_static! {
    static ref CONFIG: Config = {
        dotenv::dotenv().ok();

        let ticks_wait_time = std::env::var("VALIDATOR_TICKS_WAIT_TIME")
            .unwrap()
            .parse()
            .unwrap();

        let validation_tick_timeout = std::env::var("VALIDATOR_VALIDATION_TICK_TIMEOUT")
            .unwrap()
            .parse()
            .unwrap();

        Config {
            validation_tick_timeout: Duration::from_millis(validation_tick_timeout),
            ticks_wait_time: Duration::from_millis(ticks_wait_time),
            sentry_url: std::env::var("VALIDATOR_SENTRY_URL")
                .unwrap()
                .parse()
                .unwrap(),
        }
    };
}

struct Config {
    pub validation_tick_timeout: Duration,
    pub ticks_wait_time: Duration,
    pub sentry_url: String,
}

fn main() {
    use adapter::dummy::DummyAdapter;
    use adapter::ConfigBuilder;
    use clap::{App, Arg, SubCommand};
    use std::collections::HashMap;

    let matches = App::new("Validator worker")
        .version("0.2")
        .arg(
            Arg::with_name("single-tick")
                .short("s")
                .help("Runs the validator in single-tick mode"),
        )
        .subcommand(
            SubCommand::with_name("dummy")
                .about("Runs the validator with the Dummy adapter")
                .arg(
                    Arg::with_name("IDENTITY")
                        .help("The dummy identity to be used for the validator")
                        .required(true)
                        .index(1),
                ),
        )
        .get_matches();

    let is_single_tick = matches.is_present("single-tick");

    let adapter = match matches.subcommand_matches("dummy") {
        Some(dummy_matches) => {
            let identity = dummy_matches.value_of("IDENTITY").unwrap();

            DummyAdapter {
                config: ConfigBuilder::new(identity).build(),
                participants: HashMap::default(),
            }
        }
        None => panic!("We don't have any other adapters implemented yet!"),
    };

    run(is_single_tick, adapter);
}

fn run(is_single_tick: bool, adapter: impl Adapter) {
    use futures::future::{FutureExt, TryFutureExt};
    use reqwest::r#async::Client;

    use std::sync::Arc;
    use validator::application::validator::{Follower, Leader};
    use validator::application::worker::{InfiniteWorker, TickWorker};
    use validator::domain::worker::Worker;
    use validator::infrastructure::persistence::channel::{
        ApiChannelRepository, MemoryChannelRepository,
    };
    use validator::infrastructure::sentry::SentryApi;

    let sentry = SentryApi {
        client: Client::new(),
        sentry_url: CONFIG.sentry_url.clone(),
    };

    let _channel_repository = Arc::new(ApiChannelRepository { sentry });
    let channel_repository = Arc::new(MemoryChannelRepository::new(&[]));

    let tick_worker = TickWorker {
        leader: Leader {},
        follower: Follower {},
        channel_repository,
        identity: adapter.config().identity.to_string(),
        validation_tick_timeout: CONFIG.validation_tick_timeout,
    };

    if !is_single_tick {
        let worker = InfiniteWorker {
            tick_worker,
            ticks_wait_time: CONFIG.ticks_wait_time,
        };

        tokio::run(
            async move {
                await!(worker.run()).unwrap();
            }
                .unit_error()
                .boxed()
                .compat(),
        );
    } else {
        tokio::run(
            async move {
                await!(tick_worker.run()).unwrap();
            }
                .unit_error()
                .boxed()
                .compat(),
        );
    }
}