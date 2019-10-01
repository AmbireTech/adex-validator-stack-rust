#![deny(clippy::all)]
#![deny(rust_2018_idioms)]

use primitives::adapter::Adapter;

pub struct Application<T: Adapter> {
    // database to be initialised
    // storage: Storage,
    adapter: T,
    logger: slog::Logger,
}

impl<T: Adapter> Application<T> {
    fn new() -> Self {
        unimplemented!("whoopsy")
    }
}
