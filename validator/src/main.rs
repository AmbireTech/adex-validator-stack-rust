#![feature(async_await, await_macro)]
#![deny(rust_2018_idioms)]
#![deny(clippy::all)]

use futures::future::{FutureExt, TryFutureExt};
use reqwest::r#async::Client;

use lazy_static::lazy_static;
use std::sync::Arc;
use std::time::Duration;
use validator::domain::worker::Worker;
use validator::infrastructure::persistence::channel::api::ApiChannelRepository;
use validator::infrastructure::sentry::SentryApi;
use validator::infrastructure::validator::{Follower, Leader};
use validator::infrastructure::worker::{InfiniteWorker, TickWorker};

lazy_static! {
    static ref CONFIG: Config = {
        dotenv::dotenv().ok();

        let ticks_wait_time = std::env::var("VALIDATOR_TICKS_WAIT_TIME")
            .unwrap()
            .parse()
            .unwrap();
        Config {
            ticks_wait_time: Duration::from_millis(ticks_wait_time),
            sentry_url: std::env::var("VALIDATOR_SENTRY_URL")
                .unwrap()
                .parse()
                .unwrap(),
        }
    };
}

fn main() {
    let sentry = SentryApi {
        client: Client::new(),
        sentry_url: CONFIG.sentry_url.clone(),
    };

    let channel_repository = Arc::new(ApiChannelRepository {
        sentry: sentry.clone(),
    });

    let tick_worker = TickWorker {
        leader: Leader {},
        follower: Follower {},
        channel_repository,
    };

    let worker = InfiniteWorker { tick_worker };

    tokio::run(worker.run().boxed().compat());
}

struct Config {
    pub ticks_wait_time: Duration,
    pub sentry_url: String,
}
