use std::error::Error;

use primitives::adapter::Adapter;
use primitives::{BalancesMap, BigNum, Channel};

use crate::sentry_interface::SentryApi;
use chrono::Duration;
use chrono::Utc;
use primitives::validator::{Heartbeat, MessageTypes};

async fn send_heartbeat<A: Adapter + 'static>(iface: &SentryApi<A>) -> Result<(), Box<dyn Error>> {
    let timestamp = Utc::now();
    // TODO: create the MerkleTree

    // TODO: get the root

    // TODO: create stateRootRaw
    let state_root_raw = "state_root_raw".to_owned();
    // TODO: get the Hex of State root
    let state_root = state_root_raw.clone();
    let signature = iface.adapter.sign(&state_root_raw)?;

    iface.propagate(&[MessageTypes::Heartbeat(Heartbeat {
        signature,
        state_root,
        timestamp,
    })]);

    Ok(())
}

pub async fn heartbeat<A: Adapter + 'static>(
    iface: &SentryApi<A>,
    balances: BalancesMap,
    heartbeat_time: u32,
) -> Result<(), Box<dyn Error>> {
    let validator_message_response = await!(iface.get_our_latest_msg("Heartbeat".into()))?;

    let heartbeat_msg = validator_message_response
        .msg
        .get(0)
        .and_then(|message_types| match message_types {
            MessageTypes::Heartbeat(heartbeat) => Some(heartbeat.clone()),
            _ => None,
        });
    let should_send = match heartbeat_msg {
        Some(heartbeat) => {
            let duration = Utc::now() - heartbeat.timestamp;
            duration > Duration::milliseconds(heartbeat_time.into())
                && is_channel_not_exhausted(&iface.channel, &balances)
        }
        None => true,
    };

    if should_send {
        await!(send_heartbeat(&iface))?;
    }

    Ok(())
}

fn is_channel_not_exhausted(channel: &Channel, balances: &BalancesMap) -> bool {
    balances.values().sum::<BigNum>() == channel.deposit_amount
}
