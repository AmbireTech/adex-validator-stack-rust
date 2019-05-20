#![feature(async_await, await_macro)]

pub mod application;
pub mod domain;
pub mod infrastructure;
pub mod util;

#[cfg(test)]
pub(crate) use util::tests as test_util;