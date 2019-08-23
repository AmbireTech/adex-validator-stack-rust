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

