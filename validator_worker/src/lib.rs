#![feature(async_await, await_macro)]
#![deny(rust_2018_idioms)]
#![deny(clippy::all)]
#![allow(clippy::needless_lifetimes)]

pub mod error;
pub mod follower;
pub mod heartbeat;
pub mod leader;
pub mod producer;
pub mod sentry_interface;

pub use self::follower::Follower;
pub use self::sentry_interface::all_channels;
use crate::sentry_interface::SentryApi;
use primitives::adapter::Adapter;
use primitives::BalancesMap;

pub mod core {
    pub mod events;
    pub mod fees;
    pub mod follower_rules;
}

pub(crate) fn get_state_root_hash<A: Adapter + 'static>(
    _iface: &SentryApi<A>,
    _balances: &BalancesMap,
) -> String {
    unimplemented!("Still need implementation")
}
