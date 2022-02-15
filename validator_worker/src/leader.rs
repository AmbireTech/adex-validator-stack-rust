use thiserror::Error;

use adapter::{prelude::*, Error as AdapterError};
use primitives::{
    balances::CheckedState,
    validator::{MessageError, MessageTypes, NewState},
    Balances, ChainOf, Channel,
};

use crate::{
    heartbeat::{heartbeat, Error as HeartbeatError, HeartbeatStatus},
    sentry_interface::{Error as SentryApiError, PropagationResult, SentryApi},
    GetStateRoot, GetStateRootError,
};

#[derive(Debug)]
pub struct TickStatus {
    pub heartbeat: HeartbeatStatus,
    /// When `None` the conditions for creating a `NewState` haven't been met
    pub new_state: Option<Vec<PropagationResult>>,
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("SentryApi: {0}")]
    SentryApi(#[from] SentryApiError),
    #[error("StateRootHash: {0}")]
    StateRootHash(#[from] GetStateRootError),
    #[error("Adapter: {0}")]
    Adapter(#[from] AdapterError),
    #[error("Heartbeat: {0}")]
    Heartbeat(#[from] HeartbeatError),
    #[error("NewState Balances: {0}")]
    Message(#[from] MessageError<NewState<CheckedState>>),
    #[error("Overflow")]
    Overflow,
}

pub async fn tick<C: Unlocked + 'static>(
    sentry: &SentryApi<C>,
    channel_context: &ChainOf<Channel>,
    accounting_balances: Balances<CheckedState>,
) -> Result<TickStatus, Error> {
    let channel = channel_context.context;

    // Check if Accounting != than latest NewState (Accounting.balances != NewState.balances)
    let should_generate_new_state =
        {
            // If the accounting is empty, then we don't need to create a NewState
            if accounting_balances.earners.is_empty() || accounting_balances.spenders.is_empty() {
                false
            } else {
                let latest_new_state = sentry
                    .get_our_latest_msg(channel.id(), &["NewState"])
                    .await?
                    .map(NewState::<CheckedState>::try_from)
                    .transpose()?;

                match latest_new_state {
                    Some(new_state) => {
                        let check_spenders = accounting_balances.spenders.iter().any(
                            |(spender, accounting_balance)| {
                                match new_state.balances.spenders.get(spender) {
                                    Some(prev_balance) => accounting_balance > prev_balance,
                                    // if there is no previous balance for this Spender then it should generate a `NewState`
                                    // this includes adding an empty Spender to be included in the MerkleTree
                                    None => true,
                                }
                            },
                        );

                        let check_earners = accounting_balances.earners.iter().any(
                            |(earner, accounting_balance)| {
                                match new_state.balances.earners.get(earner) {
                                    Some(prev_balance) => accounting_balance > prev_balance,
                                    // if there is no previous balance for this Earner then it should generate a `NewState`
                                    // this includes adding an empty Earner to be included in the MerkleTree
                                    None => true,
                                }
                            },
                        );

                        check_spenders || check_earners
                    }
                    // if no previous `NewState` (i.e. `Channel` is new) - it should generate a `NewState`
                    // this is only valid if the Accounting balances are not empty!
                    None => true,
                }
            }
        };

    // Create a `NewState` if balances have changed
    let new_state = if should_generate_new_state {
        Some(on_new_accounting(sentry, channel_context, accounting_balances).await?)
    } else {
        None
    };

    Ok(TickStatus {
        heartbeat: heartbeat(sentry, channel_context).await?,
        new_state,
    })
}

async fn on_new_accounting<C: Unlocked + 'static>(
    sentry: &SentryApi<C>,
    channel_context: &ChainOf<Channel>,
    accounting_balances: Balances<CheckedState>,
) -> Result<Vec<PropagationResult>, Error> {
    let state_root = accounting_balances.encode(
        channel_context.context.id(),
        channel_context.token.precision.get(),
    )?;

    let signature = sentry.adapter.sign(&state_root)?;

    let propagation_results = sentry
        .propagate(
            channel_context,
            &[&MessageTypes::NewState(NewState {
                state_root,
                signature,
                balances: accounting_balances.into_unchecked(),
            })],
        )
        .await?;

    Ok(propagation_results)
}
