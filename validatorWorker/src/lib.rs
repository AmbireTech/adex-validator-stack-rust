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