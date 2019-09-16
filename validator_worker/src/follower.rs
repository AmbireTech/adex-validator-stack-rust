use std::error::Error;

use primitives::adapter::Adapter;
use primitives::validator::{MessageTypes, NewState};
use primitives::BalancesMap;

use crate::heartbeat::heartbeat;
use crate::sentry_interface::SentryApi;
use crate::{get_state_root_hash, producer};

pub async fn tick<A: Adapter + 'static>(iface: &SentryApi<A>) -> Result<(), Box<dyn Error>> {
    let from = iface.channel.spec.validators.leader().id.clone();
    let new_msg_response = await!(iface.get_latest_msg(from, "NewState".to_string()))?;
    let new_msg = new_msg_response
        .msg
        .get(0)
        .and_then(|message_types| match message_types {
            MessageTypes::NewState(new_state) => Some(new_state.clone()),
            _ => None,
        });
    let our_latest_msg_response =
        await!(iface.get_our_latest_msg("ApproveState+RejectState".to_string()))?;
    let our_latest_msg_state_root = our_latest_msg_response
        .msg
        .get(0)
        .and_then(|message_types| match message_types {
            MessageTypes::ApproveState(approve_state) => Some(approve_state.state_root.clone()),
            MessageTypes::RejectState(reject_state) => Some(reject_state.state_root.clone()),
            _ => None,
        });

    let latest_is_responded_to = match (&new_msg, &our_latest_msg_state_root) {
        (Some(new_msg), Some(state_root)) => &new_msg.state_root == state_root,
        (_, _) => false,
    };

    let (balances, _) = await!(producer::tick(&iface))?;

    match (new_msg, latest_is_responded_to) {
        (Some(new_state), false) => on_new_state(&iface, &balances, &new_state)?,
        (_, _) => {}
    }

    // TODO: Pass the heartbeat time from the Configuration
    await!(heartbeat(&iface, balances, 250)).map(|_| ())
}

fn on_new_state<A: Adapter + 'static>(
    _iface: &SentryApi<A>,
    _balances: &BalancesMap,
    _new_state: &NewState,
) -> Result<(), Box<dyn Error>> {
    unimplemented!();
}
