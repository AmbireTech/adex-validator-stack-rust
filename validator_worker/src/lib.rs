#![feature(async_await, await_macro)]
#![deny(rust_2018_idioms)]
#![deny(clippy::all)]
#![allow(clippy::needless_lifetimes)]

pub mod error;
pub mod follower;
pub mod leader;
pub mod sentry_interface;

pub use self::follower::Follower;
pub use self::leader::Leader;
pub use self::sentry_interface::all_channels;

pub mod core {
    pub mod fees;
    pub mod follower_rules;
}
