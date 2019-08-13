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

// pub struct Config {
//     pub validation_tick_timeout: Duration,
//     pub ticks_wait_time: Duration,
//     pub sentry_url: String,
// }

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

