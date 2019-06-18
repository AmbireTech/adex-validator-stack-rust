#![feature(async_await, await_macro)]
#![deny(rust_2018_idioms)]
#![deny(clippy::all)]

use futures::future::{FutureExt, TryFutureExt};
use reqwest::r#async::Client;

use lazy_static::lazy_static;
use validator::domain::channel::ChannelRepository;
use validator::infrastructure::persistence::channel::api::ApiChannelRepository;
use validator::infrastructure::sentry::SentryApi;

lazy_static! {
    static ref CONFIG: Config = {
        Config { ticks_wait_time: std::env::var("VALIDATOR_TICKS_WAIT_TIME").unwrap().parse().unwrap()}
    };
}

fn main() {
    let future = async {
        let sentry = SentryApi { client: Client::new(), sentry_url: "http://localhost:8005".into() };
        let repo = ApiChannelRepository { sentry };

        let all_channels = await!(repo.all("0x2892f6C41E0718eeeDd49D98D648C789668cA67d"));

        match all_channels {
            Ok(channel) => println!("{:#?}", channel),
            Err(error) => eprintln!("Error occurred: {:#?}", error),
        };
    };

    tokio::run(future.unit_error().boxed().compat());
}

struct Config { ticks_wait_time: u64 }
