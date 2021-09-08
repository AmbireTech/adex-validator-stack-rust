use std::convert::TryFrom;
use std::fmt;

use primitives::{
    adapter::{Adapter, AdapterErrorKind},
    balances::{Balances, CheckedState, UncheckedState},
    channel_v5::Channel,
    config::TokenInfo,
    validator::{ApproveState, MessageTypes, NewState, RejectState},
};

// use crate::core::follower_rules::{get_health, is_valid_transition};
use crate::{
    get_state_root_hash,
    heartbeat::{heartbeat, HeartbeatStatus},
    sentry_interface::{PropagationResult, SentryApi},
};
use chrono::Utc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("overflow placeholder")]
    Overflow,
}

#[derive(Debug)]
pub enum InvalidNewState {
    RootHash,
    Signature,
    Transition,
    Health,
}

impl fmt::Display for InvalidNewState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use InvalidNewState::*;

        let string = match self {
            RootHash => "InvalidRootHash",
            Signature => "InvalidSignature",
            Transition => "InvalidTransition",
            Health => "TooLowHealth",
        };

        write!(f, "{}", string)
    }
}

#[derive(Debug)]
pub enum ApproveStateResult<AE: AdapterErrorKind + 'static> {
    /// If None, Conditions for handling the new state haven't been met
    Sent(Option<Vec<PropagationResult<AE>>>),
    RejectedState {
        reason: InvalidNewState,
        state_root: String,
        propagation: Vec<PropagationResult<AE>>,
    },
}

#[derive(Debug)]
pub struct TickStatus<AE: AdapterErrorKind + 'static> {
    pub heartbeat: HeartbeatStatus<AE>,
    pub approve_state: ApproveStateResult<AE>,
}

pub async fn tick<A: Adapter + 'static>(
    iface: &SentryApi<A>,
    channel: Channel,
    accounting_balances: Balances<CheckedState>,
) -> Result<TickStatus<A::AdapterError>, Box<dyn std::error::Error>> {
    let from = iface.channel.leader;

    // if we don't have a `NewState` return `None`
    let new_msg = iface
        .get_latest_msg(&from, &["NewState"])
        .await?
        .and_then(|message_types| NewState::try_from(message_types).ok());

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
        _ => false,
    };

    // TODO: Use error for "Token not whitelisted for this channel"
    let token = iface
        .config
        .token_address_whitelist
        .get(&channel.token)
        .unwrap();

    let approve_state_result = if let (Some(new_state), false) = (new_msg, latest_is_responded_to) {
        on_new_state(iface, accounting_balances, &new_state, token).await?
    } else {
        ApproveStateResult::Sent(None)
    };

    Ok(TickStatus {
        heartbeat: heartbeat(iface).await?,
        approve_state: approve_state_result,
    })
}

#[allow(dead_code)]
async fn on_new_state<'a, A: Adapter + 'static>(
    iface: &'a SentryApi<A>,
    _accounting_balances: Balances<CheckedState>,
    new_state: &'a NewState<UncheckedState>,
    token_info: &TokenInfo,
) -> Result<ApproveStateResult<A::AdapterError>, Box<dyn std::error::Error>> {
    let proposed_balances = match new_state.balances.clone().check() {
        Ok(balances) => balances,

        Err(_err) => return Ok(on_error(iface, new_state, InvalidNewState::Transition).await),
    };

    let proposed_state_root = new_state.state_root.clone();

    if proposed_state_root
        != hex::encode(get_state_root_hash(
            iface.channel.id(),
            &proposed_balances,
            token_info.precision.get(),
        )?)
    {
        return Ok(on_error(iface, new_state, InvalidNewState::RootHash).await);
    }

    if !iface.adapter.verify(
        &iface.channel.leader,
        &proposed_state_root,
        &new_state.signature,
    )? {
        return Ok(on_error(iface, new_state, InvalidNewState::Signature).await);
    }

    let last_approve_response = iface.get_last_approved(iface.channel.id()).await?;
    let _prev_balances = match last_approve_response
        .last_approved
        .and_then(|last_approved| last_approved.new_state)
    {
        Some(new_state) => new_state.msg.into_inner().balances,
        _ => Default::default(),
    };

    // TODO: Check if transition is valid
    // if !is_valid_transition(
    //     &iface.channel,
    //     &BalancesMap::default(),
    //     &BalancesMap::default(),
    // ) {
    //     return Ok(on_error(iface, new_state, InvalidNewState::Transition).await);
    // }

    // let health = get_health(&iface.channel, accounting_balances, &proposed_balances);
    let health = 950;
    if health < u64::from(iface.config.health_unsignable_promilles) {
        return Ok(on_error(iface, new_state, InvalidNewState::Health).await);
    }

    let signature = iface.adapter.sign(&new_state.state_root)?;
    let health_threshold = u64::from(iface.config.health_threshold_promilles);
    let is_healthy = health >= health_threshold;

    let propagation_result = iface
        .propagate(&[&MessageTypes::ApproveState(ApproveState {
            state_root: proposed_state_root,
            signature,
            is_healthy,
        })])
        .await;

    Ok(ApproveStateResult::Sent(Some(propagation_result)))
}

#[allow(dead_code)]
async fn on_error<'a, A: Adapter + 'static>(
    iface: &'a SentryApi<A>,
    new_state: &'a NewState<UncheckedState>,
    status: InvalidNewState,
) -> ApproveStateResult<A::AdapterError> {
    let propagation = iface
        .propagate(&[&MessageTypes::RejectState(RejectState {
            reason: status.to_string(),
            state_root: new_state.state_root.clone(),
            signature: new_state.signature.clone(),
            balances: Some(new_state.balances.clone()),
            /// The NewState timestamp that is being rejected
            timestamp: Some(Utc::now()),
        })])
        .await;

    ApproveStateResult::RejectedState {
        reason: status,
        state_root: new_state.state_root.clone(),
        propagation,
    }
}

// OUTPACE Rules:
pub fn is_valid_transition(
    accounting: Balances<CheckedState>,
    new_state: Balances<CheckedState>,
) -> Result<bool, Error> {
    let (accounting_earners, accounting_spenders) = accounting.sum().ok_or(Error::Overflow)?;
    let (new_state_earners, new_state_spenders) = new_state.sum().ok_or(Error::Overflow)?;

    // sum(accounting.balances.spenders) > sum(new_state.balances.spenders)
    // sum(accounting.balances.earners) > sum(new_state.balances.earners)
    let is_valid =
        !(accounting_spenders > new_state_spenders) || !(accounting_earners > new_state_earners);

    Ok(is_valid)
}
