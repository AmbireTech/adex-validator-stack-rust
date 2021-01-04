use std::error::Error;
use std::fmt;

use primitives::adapter::{Adapter, AdapterErrorKind};
use primitives::validator::{ApproveState, MessageTypes, NewState, RejectState};
use primitives::{BalancesMap, BigNum};

use crate::core::follower_rules::{get_health, is_valid_transition};
use crate::heartbeat::{heartbeat, HeartbeatStatus};
use crate::sentry_interface::{PropagationResult, SentryApi};
use crate::{get_state_root_hash, producer};
use chrono::Utc;

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
pub enum ApproveStateResult<AE: AdapterErrorKind> {
    /// If None, Conditions for handling the new state haven't been met
    Sent(Option<Vec<PropagationResult<AE>>>),
    RejectedState {
        reason: InvalidNewState,
        state_root: String,
        propagation: Vec<PropagationResult<AE>>,
    },
}

#[derive(Debug)]
pub struct TickStatus<AE: AdapterErrorKind> {
    pub heartbeat: HeartbeatStatus<AE>,
    pub approve_state: ApproveStateResult<AE>,
    pub producer_tick: producer::TickStatus<AE>,
}

pub async fn tick<A: Adapter + 'static>(
    iface: &SentryApi<A>,
) -> Result<TickStatus<A::AdapterError>, Box<dyn Error>> {
    let from = &iface.channel.spec.validators.leader().id;
    let new_msg_response = iface.get_latest_msg(from, &["NewState"]).await?;
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
        _ => false,
    };

    let producer_tick = producer::tick(&iface).await?;
    let empty_balances = BalancesMap::default();
    let balances = match &producer_tick {
        producer::TickStatus::Sent { new_accounting, .. } => &new_accounting.balances,
        producer::TickStatus::NoNewEventAggr(balances) => balances,
        producer::TickStatus::EmptyBalances => &empty_balances,
    };
    let approve_state_result = if let (Some(new_state), false) = (new_msg, latest_is_responded_to) {
        on_new_state(&iface, &balances, &new_state).await?
    } else {
        ApproveStateResult::Sent(None)
    };

    Ok(TickStatus {
        heartbeat: heartbeat(&iface, &balances).await?,
        approve_state: approve_state_result,
        producer_tick,
    })
}

async fn on_new_state<'a, A: Adapter + 'static>(
    iface: &'a SentryApi<A>,
    balances: &'a BalancesMap,
    new_state: &'a NewState,
) -> Result<ApproveStateResult<A::AdapterError>, Box<dyn Error>> {
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
    let prev_balances = match last_approve_response
        .last_approved
        .and_then(|last_approved| last_approved.new_state)
    {
        Some(new_state) => match new_state.msg {
            MessageTypes::NewState(new_state) => new_state.balances,
            _ => Default::default(),
        },
        _ => Default::default(),
    };

    if !is_valid_transition(&iface.channel, &prev_balances, &proposed_balances) {
        return Ok(on_error(&iface, &new_state, InvalidNewState::Transition).await);
    }

    let health = get_health(&iface.channel, balances, &proposed_balances);
    if health < u64::from(iface.config.health_unsignable_promilles) {
        return Ok(on_error(&iface, &new_state, InvalidNewState::Health).await);
    }

    let signature = iface.adapter.sign(&new_state.state_root)?;
    let health_threshold = u64::from(iface.config.health_threshold_promilles);
    let is_healthy = health >= health_threshold;
    let exhausted = proposed_balances.values().sum::<BigNum>() == iface.channel.deposit_amount;

    let propagation_result = iface
        .propagate(&[&MessageTypes::ApproveState(ApproveState {
            state_root: proposed_state_root,
            signature,
            is_healthy,
            exhausted,
        })])
        .await;

    Ok(ApproveStateResult::Sent(Some(propagation_result)))
}

async fn on_error<'a, A: Adapter + 'static>(
    iface: &'a SentryApi<A>,
    new_state: &'a NewState,
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
