use std::error::Error;

use primitives::{BalancesMap, Channel, BigNum};
use primitives::adapter::Adapter;

use crate::sentry_interface::SentryApi;
use primitives::validator::MessageTypes;
use chrono::Utc;
use chrono::Duration;

async fn send_heartbeat<A: Adapter + 'static>(_iface: &SentryApi<A>) -> () {

}

pub async fn heartbeat<A: Adapter + 'static>(iface: &SentryApi<A>, balances: BalancesMap, heartbeat_time: u32) -> Result<(), Box<dyn Error>> {
    let validator_message_response = await!(iface.get_our_latest_msg("Heartbeat".into()))?;

    let heartbeat_msg = validator_message_response.msg.get(0).and_then(|message_types| {
        match message_types {
            MessageTypes::Heartbeat(heartbeat) => Some(heartbeat.clone()),
            _ => None,
        }
    });
    let should_send = match heartbeat_msg {
        Some(heartbeat) => {
            let duration = Utc::now() - heartbeat.timestamp;
            duration > Duration::milliseconds(heartbeat_time.into()) && is_channel_not_exhausted(&iface.channel, &balances)
        },
        None => true,
    };

    if should_send {
        await!(send_heartbeat(&iface));
    }

    Ok(())
}

fn is_channel_not_exhausted(channel: &Channel, balances: &BalancesMap) -> bool {
    balances.values().sum::<BigNum>() == channel.deposit_amount
}
