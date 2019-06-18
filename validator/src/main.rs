#![feature(async_await, await_macro)]
#![deny(rust_2018_idioms)]
#![deny(clippy::all)]

use futures::compat::Future01CompatExt;
use futures::future::{join, FutureExt, TryFutureExt};
use reqwest::r#async::Client;

use lazy_static::lazy_static;
use std::ops::Add;
use std::time::{Duration, Instant};
use tokio::timer::Delay;
use validator::domain::channel::ChannelRepository;
use validator::infrastructure::persistence::channel::api::ApiChannelRepository;
use validator::infrastructure::sentry::SentryApi;

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
    let worker = async {
        loop {
            let future = async {
                let sentry = SentryApi {
                    client: Client::new(),
                    sentry_url: CONFIG.sentry_url.clone(),
                };
                let repo = ApiChannelRepository { sentry };

                let all_channels = await!(repo.all("0x2892f6C41E0718eeeDd49D98D648C789668cA67d"));

                match all_channels {
                    Ok(channel) => println!("{:#?}", channel),
                    Err(error) => eprintln!("Error occurred: {:#?}", error),
                };
            };
            let tick_future = Delay::new(Instant::now().add(CONFIG.ticks_wait_time));

            let joined = join(future, tick_future.compat());

            await!(joined);
        }
    };

    tokio::run(worker.unit_error().boxed().compat());
}

struct Config {
    pub ticks_wait_time: Duration,
    pub sentry_url: String,
}
