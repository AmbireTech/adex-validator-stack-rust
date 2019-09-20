use std::error::Error;

use primitives::adapter::Adapter;
use primitives::validator::{ApproveState, MessageTypes, NewState, RejectState};
use primitives::BalancesMap;

use crate::core::follower_rules::{is_healthy, is_valid_transition};
use crate::heartbeat::heartbeat;
use crate::sentry_interface::SentryApi;
use crate::{get_state_root_hash, producer};
use chrono::Utc;

enum InvalidNewState {
    RootHash,
    Signature,
    Transition,
}

enum NewStateResult {
    Ok,
    Err(InvalidNewState),
}

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

    if let (Some(new_state), false) = (new_msg, latest_is_responded_to) {
        await!(on_new_state(&iface, &balances, &new_state))?;
    }

    await!(heartbeat(&iface, balances)).map(|_| ())
}

async fn on_new_state<'a, A: Adapter + 'static>(
    iface: &'a SentryApi<A>,
    balances: &'a BalancesMap,
    new_state: &'a NewState,
) -> Result<NewStateResult, Box<dyn Error>> {
    let proposed_balances = new_state.balances.clone();
    let proposed_state_root = new_state.state_root.clone();

    if proposed_state_root != hex::encode(get_state_root_hash(&iface, &proposed_balances)?) {
        return Ok(await!(on_error(
            &iface,
            &new_state,
            InvalidNewState::RootHash
        )));
    }

    if !iface.adapter.verify(
        &iface.channel.spec.validators.leader().id,
        &proposed_state_root,
        &new_state.signature,
    )? {
        return Ok(await!(on_error(
            &iface,
            &new_state,
            InvalidNewState::Signature
        )));
    }

    let last_approve_response = await!(iface.get_last_approved())?;
    let prev_balances = last_approve_response
        .last_approved
        .new_state
        .map(|new_state| new_state.balances)
        .unwrap_or_else(Default::default);
    if !is_valid_transition(&iface.channel, &prev_balances, &proposed_balances) {
        return Ok(await!(on_error(
            &iface,
            &new_state,
            InvalidNewState::Transition
        )));
    }

    let signature = iface.adapter.sign(&new_state.state_root)?;
    let health_threshold = u64::from(iface.config.health_threshold_promilles).into();

    iface.propagate(&[&MessageTypes::ApproveState(ApproveState {
        state_root: proposed_state_root,
        signature,
        is_healthy: is_healthy(
            &iface.channel,
            balances,
            &proposed_balances,
            &health_threshold,
        ),
    })]);

    Ok(NewStateResult::Ok)
}

async fn on_error<'a, A: Adapter + 'static>(
    iface: &'a SentryApi<A>,
    new_state: &'a NewState,
    status: InvalidNewState,
) -> NewStateResult {
    use InvalidNewState::*;
    let reason = match &status {
        RootHash => "InvalidRootHash",
        Signature => "InvalidSignature",
        Transition => "InvalidTransition",
    }
    .to_string();

    iface.propagate(&[&MessageTypes::RejectState(RejectState {
        reason,
        state_root: new_state.state_root.clone(),
        signature: new_state.signature.clone(),
        balances: Some(new_state.balances.clone()),
        /// The NewState timestamp that is being rejected
        // TODO: Double check this, if we decide to have 2 timestamps - 1 for the RejectState & 1 for NewState timestamp
        timestamp: Some(Utc::now()),
    })]);

    NewStateResult::Err(status)
}
