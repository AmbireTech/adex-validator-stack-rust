#![feature(async_await, await_macro)]
#![deny(rust_2018_idioms)]
#![deny(clippy::all)]
#![allow(clippy::needless_lifetimes)]
pub mod core {
    pub mod fees;
    pub mod follower_rules;
}
