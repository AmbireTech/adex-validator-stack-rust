#![feature(async_await, await_macro)]
#![deny(rust_2018_idioms)]
#![deny(clippy::all)]
#![allow(clippy::needless_lifetimes)]

pub mod sentry_interface;
pub mod error;
pub mod leader;
pub mod follower;

pub use self::sentry_interface::{all_channels};
pub use self::leader::Leader;
pub use self::follower::Follower;

pub mod core {
    pub mod fees;
    pub mod follower_rules;
}