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
    let new_msg_response = iface
        .get_latest_msg(from.to_string(), &["NewState"])
        .await?;
    let new_msg = match new_msg_response {
        Some(MessageTypes::NewState(new_state)) => Some(new_state),
        _ => None,
    };

    let our_latest_msg_response = iface
        .get_our_latest_msg(&["ApproveState", "RejectState"])
        .await?;

    let our_latest_msg_state_root = match our_latest_msg_response {
        Some(MessageTypes::ApproveState(approve_state)) => Some(approve_state.state_root),
        Some(MessageTypes::RejectState(reject_state)) => Some(reject_state.state_root),
        _ => None,
    };

    let latest_is_responded_to = match (&new_msg, &our_latest_msg_state_root) {
        (Some(new_msg), Some(state_root)) => &new_msg.state_root == state_root,
        (_, _) => false,
    };

    let (balances, _) = producer::tick(&iface).await?;
    if let (Some(new_state), false) = (new_msg, latest_is_responded_to) {
        on_new_state(&iface, &balances, &new_state).await?;
    }
    heartbeat(&iface, balances).await.map(|_| ())
}

async fn on_new_state<'a, A: Adapter + 'static>(
    iface: &'a SentryApi<A>,
    balances: &'a BalancesMap,
    new_state: &'a NewState,
) -> Result<NewStateResult, Box<dyn Error>> {
    let proposed_balances = new_state.balances.clone();
    let proposed_state_root = new_state.state_root.clone();
    if proposed_state_root != hex::encode(get_state_root_hash(&iface, &proposed_balances)?) {
        return Ok(on_error(&iface, &new_state, InvalidNewState::RootHash).await);
    }

    if !iface.adapter.verify(
        &iface.channel.spec.validators.leader().id,
        &proposed_state_root,
        &new_state.signature,
    )? {
        return Ok(on_error(&iface, &new_state, InvalidNewState::Signature).await);
    }

    let last_approve_response = iface.get_last_approved().await?;
    let prev_balances = last_approve_response
        .last_approved
        .and_then(|last_approved| last_approved.new_state)
        .map_or(Default::default(), |new_state| new_state.msg.balances);

    if !is_valid_transition(&iface.channel, &prev_balances, &proposed_balances) {
        return Ok(on_error(&iface, &new_state, InvalidNewState::Transition).await);
    }

    let signature = iface.adapter.sign(&new_state.state_root)?;
    let health_threshold = u64::from(iface.config.health_threshold_promilles).into();
    let health = is_healthy(
        &iface.channel,
        balances,
        &proposed_balances,
        &health_threshold,
    );

    iface
        .propagate(&[&MessageTypes::ApproveState(ApproveState {
            state_root: proposed_state_root,
            signature,
            is_healthy: health,
        })])
        .await;

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

    iface
        .propagate(&[&MessageTypes::RejectState(RejectState {
            reason,
            state_root: new_state.state_root.clone(),
            signature: new_state.signature.clone(),
            balances: Some(new_state.balances.clone()),
            /// The NewState timestamp that is being rejected
            timestamp: Some(Utc::now()),
        })])
        .await;

    NewStateResult::Err(status)
}
