use primitives::adapter::Adapter;

pub struct Application<T: Adapter, S: Storage> {
    // database to be intialised
    storage: S,
    adapter: T,
    logger: slog::Logger,
}

impl<T: Adapter, S: Storage> Application<T, S> {
    fn new() -> Self {}
}
