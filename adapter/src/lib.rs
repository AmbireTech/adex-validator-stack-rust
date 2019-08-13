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
pub mod dummy;
pub mod ethereum;

pub use self::dummy::DummyAdapter;
pub use self::ethereum::EthereumAdapter;

pub fn signable_state_root(
        channel_id: &str,
        balance_root: &str,
) -> String {
        // @TODO
        "Signed".to_string()
    
}

// fn get_balance_leaf()