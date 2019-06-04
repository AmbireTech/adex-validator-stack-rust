#![deny(rust_2018_idioms)]
#![deny(clippy::all)]
pub use self::adapter::*;
pub use self::sanity::*;

mod adapter;
#[cfg(any(test, feature = "dummy-adapter"))]
pub mod dummy;
mod sanity;
