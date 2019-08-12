#![feature(async_await, await_macro)]
#![deny(rust_2018_idioms)]
#![deny(clippy::all)]
#![allow(clippy::needless_lifetimes)]

use std::time::Duration;
use primitives::config::{Config, PRODUCTION_CONFIG, DEVELOPMENT_CONFIG};

// pub struct Config {
//     pub validation_tick_timeout: Duration,
//     pub ticks_wait_time: Duration,
//     pub sentry_url: String,
// }

pub fn config(environment: str, config_file: Option<str>) -> Config {
    

}