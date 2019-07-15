use chrono::{Duration, Utc};
use std::convert::TryFrom;
use std::error::Error;
use std::fmt;

use adapter::{Adapter, AdapterError, BalanceRoot, ChannelId as AdapterChannelId};
use domain::validator::message::{Heartbeat, Message, State, TYPE_HEARTBEAT};
use domain::{Channel, ChannelId, RepositoryError, ValidatorId};

use crate::domain::MessageRepository;

pub struct HeartbeatFactory<A: Adapter> {
    adapter: A,
}

#[derive(Debug)]
pub enum HeartbeatError {
    Adapter(AdapterError),
    Repository(RepositoryError),
    /// When the Channel deposit has been exhausted
    ChannelExhausted(ChannelId),
    /// When the required time for the Heartbeat delay hasn't passed
    NotYetTime,
    User(String),
}

impl Error for HeartbeatError {}

impl fmt::Display for HeartbeatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HeartbeatError::Adapter(error) => write!(f, "Adapter error: {}", error),
            HeartbeatError::Repository(error) => write!(f, "Repository error: {}", error),
            HeartbeatError::ChannelExhausted(channel_id) => {
                write!(f, "Channel {} exhausted", channel_id)
            }
            HeartbeatError::NotYetTime => write!(f, "It's not time for the heartbeat yet"),
            HeartbeatError::User(err_string) => write!(f, "User error: {}", err_string),
        }
    }
}

impl<A: Adapter> HeartbeatFactory<A> {
    pub async fn create(
        &self,
        state_root: <A::State as State>::StateRoot,
    ) -> Result<Heartbeat<A::State>, HeartbeatError> {
        let signature = await!(self.adapter.sign(&state_root)).map_err(HeartbeatError::Adapter)?;

        Ok(Heartbeat::new(signature, state_root))
    }
}

pub struct HeartbeatSender<A: Adapter> {
    message_repository: Box<dyn MessageRepository<A::State>>,
    adapter: A,
    factory: HeartbeatFactory<A>,
    // @TODO: Add config value for Heartbeat send frequency
}

impl<A: Adapter> HeartbeatSender<A> {
    pub async fn conditional_send(&self, channel: Channel) -> Result<(), HeartbeatError> {
        // get latest Heartbeat message from repo
        // TODO: Handle this error, removing this ValidatorId from here
        let validator = ValidatorId::try_from(self.adapter.config().identity.as_ref()).unwrap();
        let latest_future =
            self.message_repository
                .latest(&channel.id, &validator, Some(&[&TYPE_HEARTBEAT]));
        let latest_heartbeat = await!(latest_future)
            .map_err(HeartbeatError::Repository)?
            .map(|heartbeat_msg| match heartbeat_msg {
                Message::Heartbeat(h) => Ok(h),
                _ => Err(HeartbeatError::User(
                    "The repository returned a non-Heartbeat message".to_string(),
                )),
            })
            .transpose()?;

        // if it doesn't exist or the Passed time is greater than the Timer Time
        match latest_heartbeat.as_ref() {
            Some(heartbeat) if !self.is_heartbeat_time(heartbeat) => {
                return Err(HeartbeatError::NotYetTime)
            }
            _ => (),
        }

        // @TODO: Figure out where the channel `is_exhausted` should be located and handled.
        // check if channel is not exhausted

        let adapter_channel_id = AdapterChannelId(channel.id.bytes);
        // @TODO: Use the appropriate BalanceRoot
        let adapter_balance_root = BalanceRoot(*b"12345678901234567890123456789012");
        let signable_state_root = A::signable_state_root(adapter_channel_id, adapter_balance_root);
        // call the HeartbeatFactory and create the new Heartbeat
        let heartbeat = await!(self.factory.create(signable_state_root.0))?;

        // @TODO: Issue #93 - this should propagate the message to all validators!
        // `add()` the heartbeat with the Repository
        await!(self
            .message_repository
            .add(&channel.id, &validator, Message::Heartbeat(heartbeat)))
        .map_err(HeartbeatError::Repository)
    }

    fn is_heartbeat_time(&self, latest_heartbeat: &Heartbeat<A::State>) -> bool {
        // @TODO: Use the configuration value for the duration!
        latest_heartbeat.timestamp - Utc::now() >= Duration::seconds(10)
    }
}

#[cfg(test)]
mod test {
    use std::collections::HashMap;

    use chrono::Utc;

    use adapter::dummy::DummyAdapter;
    use adapter::ConfigBuilder;

    use super::*;

    #[test]
    fn creates_heartbeat() {
        futures::executor::block_on(async {
            let adapter = DummyAdapter {
                config: ConfigBuilder::new("identity").build(),
                participants: HashMap::default(),
            };

            let factory = HeartbeatFactory { adapter };

            let state_root = "my dummy StateRoot".into();

            let adapter_signature = await!(factory.adapter.sign(&state_root))
                .expect("Adapter should sign the StateRoot");
            let heartbeat =
                await!(factory.create(state_root)).expect("Heartbeat should be created");

            assert!(Utc::now() >= heartbeat.timestamp);
            assert_eq!(adapter_signature, heartbeat.signature);
        });
    }
}
