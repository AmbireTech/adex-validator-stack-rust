use std::error::Error;
use std::fmt;

use adapter::{Adapter, AdapterError};
use domain::validator::message::{Heartbeat, State};

pub struct HeartbeatFactory<A: Adapter + State> {
    adapter: A,
}

#[derive(Debug)]
pub enum HeartbeatFactoryError {
    Adapter(AdapterError),
}

impl Error for HeartbeatFactoryError {}

impl fmt::Display for HeartbeatFactoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HeartbeatFactoryError::Adapter(error) => write!(f, "Adapter error: {}", error),
        }
    }
}

impl<A: Adapter + State> HeartbeatFactory<A> {
    #[allow(clippy::needless_lifetimes)]
    pub async fn create<S>(&self, state_root: S) -> Result<Heartbeat<A>, HeartbeatFactoryError>
    where
        S: Into<A::StateRoot>,
    {
        let state_root = state_root.into();
        let signature =
            await!(self.adapter.sign(&state_root)).map_err(HeartbeatFactoryError::Adapter)?;

        Ok(Heartbeat::new(signature, state_root))
    }
}
