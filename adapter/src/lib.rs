#![deny(rust_2018_idioms)]
#![deny(clippy::all)]
#![doc(test(attr(cfg(feature = "dummy-adapter"))))]
pub use self::adapter::*;
pub use self::sanity::*;

mod adapter;
#[cfg(any(test, feature = "dummy-adapter"))]
pub mod dummy;
mod sanity;
