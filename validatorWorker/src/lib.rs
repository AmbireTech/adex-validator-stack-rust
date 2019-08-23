#![feature(async_await, await_macro)]
#![deny(rust_2018_idioms)]
#![deny(clippy::all)]
#![allow(clippy::needless_lifetimes)]

pub mod error;

use error::ValidatorWokerError;
use futures::future::{FutureExt, TryFutureExt};
use primitives::config::{Config, DEVELOPMENT_CONFIG, PRODUCTION_CONFIG};
use reqwest::r#async::Client;
use std::error::Error;
use std::fs;
use std::io;
use std::io::prelude::*;
use std::time::Duration;
use toml;
