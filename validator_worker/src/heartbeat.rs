use std::convert::TryFrom;
use std::error::Error;

use chrono::{Duration, Utc};

use adapter::get_signable_state_root;
use byteorder::{BigEndian, ByteOrder};
use primitives::adapter::Adapter;
use primitives::merkle_tree::MerkleTree;
use primitives::validator::{Heartbeat, MessageTypes};
use primitives::{BalancesMap, BigNum, Channel};

use crate::sentry_interface::{PropagationResult, SentryApi};

pub type HeartbeatStatus<A> = Option<Vec<PropagationResult<A>>>;

async fn send_heartbeat<A: Adapter + 'static>(
    iface: &SentryApi<A>,
) -> Result<Vec<PropagationResult<A::AdapterError>>, Box<dyn Error>> {
    let mut timestamp_buf = [0_u8; 32];
    let milliseconds: u64 = u64::try_from(Utc::now().timestamp_millis())
        .expect("The timestamp should be able to be converted to u64");
    BigEndian::write_uint(&mut timestamp_buf[26..], milliseconds, 6);

    let merkle_tree = MerkleTree::new(&[timestamp_buf])?;

    let state_root_raw = get_signable_state_root(iface.channel.id.as_ref(), &merkle_tree.root())?;
    let state_root = hex::encode(state_root_raw);

    let signature = iface.adapter.sign(&state_root)?;

    let message_types = MessageTypes::Heartbeat(Heartbeat {
        signature,
        state_root,
        timestamp: Utc::now(),
    });

    Ok(iface.propagate(&[&message_types]).await)
}

pub async fn heartbeat<A: Adapter + 'static>(
    iface: &SentryApi<A>,
    balances: &BalancesMap,
) -> Result<HeartbeatStatus<A::AdapterError>, Box<dyn Error>> {
    let validator_message_response = iface.get_our_latest_msg(&["Heartbeat"]).await?;
    let heartbeat_msg = match validator_message_response {
        Some(MessageTypes::Heartbeat(heartbeat)) => Some(heartbeat),
        _ => None,
    };

    let should_send = heartbeat_msg.map_or(true, |heartbeat| {
        let duration = Utc::now() - heartbeat.timestamp;
        duration > Duration::milliseconds(iface.config.heartbeat_time.into())
            && !is_channel_exhausted(&iface.channel, balances)
    });

    if should_send {
        Ok(Some(send_heartbeat(&iface).await?))
    } else {
        Ok(None)
    }
}

fn is_channel_exhausted(channel: &Channel, balances: &BalancesMap) -> bool {
    balances.values().sum::<BigNum>() == channel.deposit_amount
}
