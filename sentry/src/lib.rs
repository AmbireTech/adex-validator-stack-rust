#[macro_use()]
use slog::{crit, debug, info, o, Drain};

pub Application<T: Adapter, S: Storage> {
    // database to be intialised
    storage: S,
    adapter: T,
    logger: slog::Logger,
}

impl Application {
    fn new() -> Self {

    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
