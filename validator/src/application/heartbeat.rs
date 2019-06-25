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
    pub async fn create(
        &self,
        state_root: A::StateRoot,
    ) -> Result<Heartbeat<A>, HeartbeatFactoryError> {
        let signature =
            await!(self.adapter.sign(&state_root)).map_err(HeartbeatFactoryError::Adapter)?;

        Ok(Heartbeat::new(signature, state_root))
    }
}

#[cfg(test)]
mod test {
    use std::collections::HashMap;

    use adapter::dummy::DummyAdapter;
    use adapter::ConfigBuilder;

    use super::*;
    use chrono::Utc;

    #[test]
    fn creates_heartbeat() {
        futures::executor::block_on(async {
            let adapter = DummyAdapter {
                config: ConfigBuilder::new("identity").build(),
                participants: HashMap::default(),
            };

            let factory = HeartbeatFactory { adapter };

            let state_root = "my dummy StateRoot".to_string();

            let adapter_signature = await!(factory.adapter.sign(&state_root)).expect("Adapter should sign the StateRoot");
            let heartbeat =
                await!(factory.create(state_root)).expect("Heartbeat should be created");

            assert!(Utc::now() >= heartbeat.timestamp);
            assert_eq!(adapter_signature, heartbeat.signature);
        });
    }
}
