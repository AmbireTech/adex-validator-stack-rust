use std::convert::TryFrom;
use thiserror::Error;

use primitives::{
    adapter::{Adapter, AdapterErrorKind, Error as AdapterError},
    balances::CheckedState,
    channel::Channel,
    config::TokenInfo,
    validator::{MessageError, MessageTypes, NewState},
    Balances, ChannelId,
};

use crate::{
    get_state_root_hash,
    heartbeat::{heartbeat, Error as HeartbeatError, HeartbeatStatus},
    sentry_interface::{Error as SentryApiError, PropagationResult, SentryApi},
    StateRootHashError,
};

#[derive(Debug)]
pub struct TickStatus {
    pub heartbeat: HeartbeatStatus,
    /// When `None` the conditions for creating a `NewState` haven't been met
    pub new_state: Option<Vec<PropagationResult>>,
}

#[derive(Debug, Error)]
pub enum Error<AE: AdapterErrorKind + 'static> {
    #[error("SentryApi: {0}")]
    SentryApi(#[from] SentryApiError),
    #[error("StateRootHash: {0}")]
    StateRootHash(#[from] StateRootHashError),
    #[error("Adapter: {0}")]
    Adapter(#[from] AdapterError<AE>),
    #[error("Heartbeat: {0}")]
    Heartbeat(#[from] HeartbeatError<AE>),
    #[error("NewState Balances: {0}")]
    Message(#[from] MessageError<NewState<CheckedState>>),
    #[error("Overflow")]
    Overflow,
}

pub async fn tick<A: Adapter + 'static>(
    sentry: &SentryApi<A>,
    channel: Channel,
    accounting_balances: Balances<CheckedState>,
    token: &TokenInfo,
) -> Result<TickStatus, Error<A::AdapterError>> {
    // Check if Accounting != than latest NewState (Accounting.balances != NewState.balances)
    let should_generate_new_state = {
        let latest_new_state = sentry
            .get_our_latest_msg(channel.id(), &["NewState"])
            .await?
            .map(NewState::<CheckedState>::try_from)
            .transpose()?;

        match latest_new_state {
            Some(new_state) => {
                let check_spenders =
                    accounting_balances
                        .spenders
                        .iter()
                        .any(|(spender, accounting_balance)| {
                            match new_state.balances.spenders.get(spender) {
                                Some(prev_balance) => accounting_balance > prev_balance,
                                // if there is no previous balance for this Spender then it should generate a `NewState`
                                // this includes adding an empty Spender to be included in the MerkleTree
                                None => true,
                            }
                        });

                let check_earners =
                    accounting_balances
                        .earners
                        .iter()
                        .any(|(earner, accounting_balance)| {
                            match new_state.balances.earners.get(earner) {
                                Some(prev_balance) => accounting_balance > prev_balance,
                                // if there is no previous balance for this Earner then it should generate a `NewState`
                                // this includes adding an empty Earner to be included in the MerkleTree
                                None => true,
                            }
                        });

                check_spenders || check_earners
            }
            // if no previous `NewState` (i.e. `Channel` is new) - it should generate a `NewState`
            None => true,
        }
    };

    // Create a `NewState` if balances have changed
    let new_state = if should_generate_new_state {
        Some(on_new_accounting(sentry, channel.id(), accounting_balances, token).await?)
    } else {
        None
    };

    Ok(TickStatus {
        heartbeat: heartbeat(sentry, channel.id()).await?,
        new_state,
    })
}

async fn on_new_accounting<A: Adapter + 'static>(
    sentry: &SentryApi<A>,
    channel: ChannelId,
    accounting_balances: Balances<CheckedState>,
    token: &TokenInfo,
) -> Result<Vec<PropagationResult>, Error<A::AdapterError>> {
    let state_root_raw = get_state_root_hash(channel, &accounting_balances, token.precision.get())?;
    let state_root = hex::encode(state_root_raw);

    let signature = sentry.adapter.sign(&state_root)?;

    let propagation_results = sentry
        .propagate(
            channel,
            &[&MessageTypes::NewState(NewState {
                state_root,
                signature,
                balances: accounting_balances.into_unchecked(),
            })],
        )
        .await;

    Ok(propagation_results)
}
