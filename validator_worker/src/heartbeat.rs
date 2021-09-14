use std::convert::TryFrom;

use chrono::{Duration, Utc};

use adapter::get_signable_state_root;
use byteorder::{BigEndian, ByteOrder};
use primitives::{ChannelId, adapter::{Adapter, AdapterErrorKind, Error as AdapterError}, merkle_tree::MerkleTree, validator::{Heartbeat, MessageTypes}};
use thiserror::Error;

use crate::sentry_interface::{Error as SentryApiError, PropagationResult, SentryApi};

pub type HeartbeatStatus = Option<Vec<PropagationResult>>;

#[derive(Debug, Error)]
pub enum Error<AE: AdapterErrorKind + 'static> {
    #[error("MerkleTree: {0}")]
    MerkleTree(#[from] primitives::merkle_tree::Error),
    #[error("Adapter error: {0}")]
    Adapter(#[from] AdapterError<AE>),
    #[error("Sentry API: {0}")]
    SentryApi(#[from] SentryApiError),
}

pub async fn heartbeat<A: Adapter + 'static>(
    iface: &SentryApi<A>,
    channel: ChannelId,
) -> Result<HeartbeatStatus, Error<A::AdapterError>> {
    let validator_message_response = iface.get_our_latest_msg(channel, &["Heartbeat"]).await?;
    let heartbeat_msg = match validator_message_response {
        Some(MessageTypes::Heartbeat(heartbeat)) => Some(heartbeat),
        _ => None,
    };

    let should_send = heartbeat_msg.map_or(true, |heartbeat| {
        let duration = Utc::now() - heartbeat.timestamp;
        duration > Duration::milliseconds(iface.config.heartbeat_time.into())
    });

    if should_send {
        Ok(Some(send_heartbeat(iface, channel).await?))
    } else {
        Ok(None)
    }
}

async fn send_heartbeat<A: Adapter + 'static>(
    iface: &SentryApi<A>,
    channel: ChannelId,
) -> Result<Vec<PropagationResult>, Error<A::AdapterError>> {
    let mut timestamp_buf = [0_u8; 32];
    let milliseconds: u64 = u64::try_from(Utc::now().timestamp_millis())
        .expect("The timestamp should be able to be converted to u64");
    BigEndian::write_uint(&mut timestamp_buf[26..], milliseconds, 6);

    let merkle_tree = MerkleTree::new(&[timestamp_buf])?;

    let state_root_raw = get_signable_state_root(channel.as_ref(), &merkle_tree.root());
    let state_root = hex::encode(state_root_raw);

    let signature = iface.adapter.sign(&state_root)?;

    let message_types = MessageTypes::Heartbeat(Heartbeat {
        signature,
        state_root,
        timestamp: Utc::now(),
    });

    Ok(iface.propagate(channel, &[&message_types]).await)
}