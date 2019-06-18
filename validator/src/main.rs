#![feature(async_await, await_macro)]
#![deny(rust_2018_idioms)]
#![deny(clippy::all)]

use futures::future::{FutureExt, TryFutureExt};
use reqwest::r#async::Client;

use validator::domain::channel::ChannelRepository;
use validator::infrastructure::persistence::channel::api::ApiChannelRepository;
use validator::infrastructure::sentry::SentryApi;

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
