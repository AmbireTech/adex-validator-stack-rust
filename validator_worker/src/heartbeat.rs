use chrono::{Duration, Utc};

use adapter::{prelude::*, util::get_signable_state_root, Error as AdapterError};
use byteorder::{BigEndian, ByteOrder};
use primitives::{
    merkle_tree::MerkleTree,
    validator::{Heartbeat, MessageTypes},
    ChainOf, Channel,
};
use thiserror::Error;

use crate::sentry_interface::{Error as SentryApiError, PropagationResult, SentryApi};

pub type HeartbeatStatus = Option<Vec<PropagationResult>>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("MerkleTree: {0}")]
    MerkleTree(#[from] primitives::merkle_tree::Error),
    #[error("Adapter error: {0}")]
    Adapter(#[from] AdapterError),
    #[error("Sentry API: {0}")]
    SentryApi(#[from] SentryApiError),
}

pub async fn heartbeat<C: Unlocked + 'static>(
    iface: &SentryApi<C>,
    channel_context: &ChainOf<Channel>,
) -> Result<HeartbeatStatus, Error> {
    let validator_message_response = iface
        .get_our_latest_msg(channel_context.context.id(), &["Heartbeat"])
        .await?;
    let heartbeat_msg = match validator_message_response {
        Some(MessageTypes::Heartbeat(heartbeat)) => Some(heartbeat),
        _ => None,
    };

    let should_send = heartbeat_msg.map_or(true, |heartbeat| {
        let duration = Utc::now() - heartbeat.timestamp;
        duration > Duration::milliseconds(iface.config.heartbeat_time.into())
    });

    if should_send {
        Ok(Some(send_heartbeat(iface, channel_context).await?))
    } else {
        Ok(None)
    }
}

async fn send_heartbeat<C: Unlocked + 'static>(
    iface: &SentryApi<C>,
    channel_context: &ChainOf<Channel>,
) -> Result<Vec<PropagationResult>, Error> {
    let mut timestamp_buf = [0_u8; 32];
    let milliseconds: u64 = u64::try_from(Utc::now().timestamp_millis())
        .expect("The timestamp should be able to be converted to u64");
    BigEndian::write_uint(&mut timestamp_buf[26..], milliseconds, 6);

    let merkle_tree = MerkleTree::new(&[timestamp_buf])?;

    let state_root_raw =
        get_signable_state_root(channel_context.context.id().as_ref(), &merkle_tree.root());
    let state_root = hex::encode(state_root_raw);

    let signature = iface.adapter.sign(&state_root)?;

    let message_types = MessageTypes::Heartbeat(Heartbeat {
        signature,
        state_root,
        timestamp: Utc::now(),
    });

    Ok(iface.propagate(channel_context, &[message_types]).await?)
}
