use std::{collections::HashMap, convert::TryFrom, fmt};

use primitives::{
    adapter::{Adapter, AdapterErrorKind, Error as AdapterError},
    balances,
    balances::{Balances, CheckedState, UncheckedState},
    channel::Channel,
    config::TokenInfo,
    spender::Spender,
    validator::{ApproveState, MessageTypes, NewState, RejectState},
    Address, ChannelId, UnifiedNum,
};

use crate::{
    GetStateRootError, GetStateRoot, core::follower_rules::{get_health, is_valid_transition},
    heartbeat::{heartbeat, HeartbeatStatus},
    sentry_interface::{Error as SentryApiError, PropagationResult, SentryApi},
};
use chrono::Utc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error<AE: AdapterErrorKind + 'static> {
    #[error("overflow placeholder")]
    Overflow,
    #[error("The Channel's Token is not whitelisted")]
    TokenNotWhitelisted,
    #[error("Couldn't get state root hash of the proposed balances")]
    StateRootHash(#[from] GetStateRootError),
    #[error("Adapter error: {0}")]
    Adapter(#[from] AdapterError<AE>),
    #[error("Sentry API: {0}")]
    SentryApi(#[from] SentryApiError),
    #[error("Heartbeat: {0}")]
    Heartbeat(#[from] crate::heartbeat::Error<AE>),
}

#[derive(Debug)]
pub enum InvalidNewState {
    RootHash,
    Signature,
    Transition,
    Health(Health),
}

#[derive(Debug)]
pub enum Health {
    Earners(u64),
    Spenders(u64),
}

impl fmt::Display for InvalidNewState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let string = match self {
            InvalidNewState::RootHash => "InvalidRootHash",
            InvalidNewState::Signature => "InvalidSignature",
            InvalidNewState::Transition => "InvalidTransition",
            // TODO: Should we use health value?
            InvalidNewState::Health(health) => match health {
                Health::Earners(_health) => "TooLowHealthEarners",
                Health::Spenders(_health) => "TooLowHealthSpenders",
            },
        };

        write!(f, "{}", string)
    }
}

#[derive(Debug)]
pub enum ApproveStateResult {
    /// When `None` the conditions for approving the `NewState` (and generating `ApproveState`) haven't been met
    Sent(Option<Vec<PropagationResult>>),
    RejectedState {
        reason: InvalidNewState,
        state_root: String,
        propagation: Vec<PropagationResult>,
    },
}

#[derive(Debug)]
pub struct TickStatus {
    pub heartbeat: HeartbeatStatus,
    pub approve_state: ApproveStateResult,
}

pub async fn tick<A: Adapter + 'static>(
    sentry: &SentryApi<A>,
    channel: Channel,
    all_spenders: HashMap<Address, Spender>,
    accounting_balances: Balances<CheckedState>,
    token: &TokenInfo,
) -> Result<TickStatus, Error<A::AdapterError>> {
    let from = channel.leader;
    let channel_id = channel.id();

    // TODO: Context for All spender sum Error when overflow occurs
    let all_spenders_sum = all_spenders
        .values()
        .map(|spender| &spender.total_deposited)
        .sum::<Option<_>>()
        .ok_or(Error::Overflow)?;

    // if we don't have a `NewState` return `None`
    let new_msg = sentry
        .get_latest_msg(channel_id, from, &["NewState"])
        .await?
        .map(NewState::try_from)
        .transpose()
        .expect("Should always return a NewState message");

    let our_latest_msg_response = sentry
        .get_our_latest_msg(channel_id, &["ApproveState", "RejectState"])
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

    let approve_state_result = if let (Some(new_state), false) = (new_msg, latest_is_responded_to) {
        on_new_state(
            sentry,
            channel,
            accounting_balances,
            new_state,
            token,
            all_spenders_sum,
        )
        .await?
    } else {
        ApproveStateResult::Sent(None)
    };

    Ok(TickStatus {
        heartbeat: heartbeat(sentry, channel_id).await?,
        approve_state: approve_state_result,
    })
}

async fn on_new_state<'a, A: Adapter + 'static>(
    sentry: &'a SentryApi<A>,
    channel: Channel,
    accounting_balances: Balances<CheckedState>,
    new_state: NewState<UncheckedState>,
    token_info: &TokenInfo,
    all_spenders_sum: UnifiedNum,
) -> Result<ApproveStateResult, Error<A::AdapterError>> {
    let proposed_balances = match new_state.balances.clone().check() {
        Ok(balances) => balances,
        // TODO: Should we show the Payout Mismatch between Spent & Earned?
        Err(balances::Error::PayoutMismatch { .. }) => {
            return Ok(on_error(sentry, channel.id(), new_state, InvalidNewState::Transition).await)
        }
        // TODO: Add context for `proposed_balances.check()` overflow error
        Err(_) => return Err(Error::Overflow),
    };

    let proposed_state_root = new_state.state_root.clone();


    if proposed_state_root
        != proposed_balances.encode(channel.id(), token_info.precision.get())?
    {
        return Ok(on_error(sentry, channel.id(), new_state, InvalidNewState::RootHash).await);
    }

    if !sentry
        .adapter
        .verify(channel.leader, &proposed_state_root, &new_state.signature)?
    {
        return Ok(on_error(sentry, channel.id(), new_state, InvalidNewState::Signature).await);
    }

    let last_approve_response = sentry.get_last_approved(channel.id()).await?;
    let prev_balances = match last_approve_response
        .last_approved
        .and_then(|last_approved| last_approved.new_state)
        .map(|new_state| new_state.msg.into_inner().balances.check())
        .transpose()
    {
        Ok(Some(previous_balances)) => previous_balances,
        Ok(None) => Default::default(),
        // TODO: Add Context for Transition error
        Err(_err) => {
            return Ok(on_error(sentry, channel.id(), new_state, InvalidNewState::Transition).await)
        }
    };

    // OUTPACE rules:
    // 1. Check the transition of previous and proposed Spenders maps:
    //
    // sum(accounting.balances.spenders) > sum(new_state.balances.spenders)
    // & Each spender value in `next` should be > the corresponding `prev` value
    if !is_valid_transition(
        all_spenders_sum,
        &prev_balances.spenders,
        &proposed_balances.spenders,
    )
    .ok_or(Error::Overflow)?
    {
        // TODO: Add context for error in Spenders transition
        return Ok(on_error(sentry, channel.id(), new_state, InvalidNewState::Transition).await);
    }

    // 2. Check the transition of previous and proposed Earners maps
    //
    // sum(accounting.balances.earners) > sum(new_state.balances.earners)
    // & Each spender value in `next` should be > the corresponding `prev` value
    // sum(accounting.balances.spenders) > sum(new_state.balances.spenders)
    if !is_valid_transition(
        all_spenders_sum,
        &prev_balances.earners,
        &proposed_balances.earners,
    )
    .ok_or(Error::Overflow)?
    {
        // TODO: Add context for error in Earners transition
        return Ok(on_error(sentry, channel.id(), new_state, InvalidNewState::Transition).await);
    }

    let health_earners = get_health(
        all_spenders_sum,
        &accounting_balances.earners,
        &proposed_balances.earners,
    )
    .ok_or(Error::Overflow)?;
    if health_earners < u64::from(sentry.config.health_unsignable_promilles) {
        return Ok(on_error(
            sentry,
            channel.id(),
            new_state,
            InvalidNewState::Health(Health::Earners(health_earners)),
        )
        .await);
    }

    let health_spenders = get_health(
        all_spenders_sum,
        &accounting_balances.spenders,
        &proposed_balances.spenders,
    )
    .ok_or(Error::Overflow)?;
    if health_spenders < u64::from(sentry.config.health_unsignable_promilles) {
        return Ok(on_error(
            sentry,
            channel.id(),
            new_state,
            InvalidNewState::Health(Health::Spenders(health_spenders)),
        )
        .await);
    }

    let signature = sentry.adapter.sign(&new_state.state_root)?;
    let health_threshold = u64::from(sentry.config.health_threshold_promilles);
    let is_healthy = health_earners >= health_threshold && health_spenders >= health_threshold;

    let propagation_result = sentry
        .propagate(
            channel.id(),
            &[&MessageTypes::ApproveState(ApproveState {
                state_root: proposed_state_root,
                signature,
                is_healthy,
            })],
        )
        .await;

    Ok(ApproveStateResult::Sent(Some(propagation_result)))
}

async fn on_error<'a, A: Adapter + 'static>(
    sentry: &'a SentryApi<A>,
    channel: ChannelId,
    new_state: NewState<UncheckedState>,
    status: InvalidNewState,
) -> ApproveStateResult {
    let propagation = sentry
        .propagate(
            channel,
            &[&MessageTypes::RejectState(RejectState {
                reason: status.to_string(),
                state_root: new_state.state_root.clone(),
                signature: new_state.signature.clone(),
                balances: Some(new_state.balances.clone()),
                /// The timestamp when the NewState is being rejected
                timestamp: Some(Utc::now()),
            })],
        )
        .await;

    ApproveStateResult::RejectedState {
        reason: status,
        state_root: new_state.state_root.clone(),
        propagation,
    }
}
