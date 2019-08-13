#![feature(async_await, await_macro)]
#![deny(rust_2018_idioms)]
#![deny(clippy::all)]
#![allow(clippy::needless_lifetimes)]

pub mod error;

use std::time::Duration;
use std::error::Error;
use std::io::prelude::*;
use std::io;
use std::fs;
use toml;
use error::{ ValidatorWokerError };
use primitives::config::{Config, PRODUCTION_CONFIG, DEVELOPMENT_CONFIG};
use futures::future::{FutureExt, TryFutureExt};
use reqwest::r#async::Client;


pub fn configuration(environment: &str, config_file: Option<&str>) -> Result<Config, ValidatorWokerError>  {
    let result : Config = match config_file {
        Some(config_file) => {
            let data = match fs::read_to_string(config_file) {
                Ok(result) => result,
                Err(e) => return Err(ValidatorWokerError::ConfigurationError(format!("Unable to read provided config file {}", config_file))),
            };
            toml::from_str(&data).unwrap()
        },
        None => {
            if environment == "production" {
                return toml::from_str(&PRODUCTION_CONFIG).unwrap();
            } else {
                return toml::from_str(&DEVELOPMENT_CONFIG).unwrap();
            }
        }
    };
    Ok(result)
}

//     use std::sync::Arc;
//     use validator::application::validator::{Follower, Leader};
//     use validator::application::worker::{InfiniteWorker, TickWorker};
//     use validator::domain::worker::Worker;
//     use validator::infrastructure::persistence::channel::{
//         ApiChannelRepository, MemoryChannelRepository,
//     };
//     use validator::infrastructure::sentry::SentryApi;

//     let sentry = SentryApi {
//         client: Client::new(),
//         sentry_url: CONFIG.sentry_url.clone(),
//     };

//     let _channel_repository = Arc::new(ApiChannelRepository { sentry });
//     let channel_repository = Arc::new(MemoryChannelRepository::new(&[]));

//     let tick_worker = TickWorker {
//         leader: Leader {},
//         follower: Follower {},
//         channel_repository,
//         identity: adapter.config().identity.to_string(),
//         validation_tick_timeout: CONFIG.validation_tick_timeout,
//     };

//     if !is_single_tick {
//         let worker = InfiniteWorker {
//             tick_worker,
//             ticks_wait_time: CONFIG.ticks_wait_time,
//         };

//         tokio::run(
//             async move {
//                 await!(worker.run()).unwrap();
//             }
//                 .unit_error()
//                 .boxed()
//                 .compat(),
//         );
//     } else {
//         tokio::run(
//             async move {
//                 await!(tick_worker.run()).unwrap();
//             }
//                 .unit_error()
//                 .boxed()
//                 .compat(),
//         );
//     }
// }