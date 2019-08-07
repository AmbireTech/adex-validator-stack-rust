#![feature(async_await, await_macro)]
#![deny(rust_2018_idioms)]
#![deny(clippy::all)]
#![doc(test(attr(feature(async_await, await_macro))))]
#![doc(test(attr(cfg(feature = "dummy-adapter"))))]
//pub use self::adapter::*;
//pub use self::sanity::*;
//
//mod adapter;
//#[cfg(any(test, feature = "dummy-adapter"))]
//pub mod dummy;
//mod sanity;
