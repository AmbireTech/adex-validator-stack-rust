use std::error::Error;

use primitives::{channel_v5::Channel, Balances, adapter::Adapter, balances::{CheckedState, UncheckedState}, config::TokenInfo, sentry::AccountingResponse, validator::{MessageTypes, NewState}};

use crate::get_state_root_hash;
use crate::heartbeat::{heartbeat, HeartbeatStatus};
use crate::sentry_interface::{PropagationResult, SentryApi};

#[derive(Debug)]
pub struct TickStatus {
    pub heartbeat: HeartbeatStatus,
    /// When `None` the conditions for creating a NewState haven't been met
    pub new_state: Option<Vec<PropagationResult>>,
}

pub async fn tick<A: Adapter + 'static>(
    sentry: &SentryApi<A>,
    channel: Channel,
    accounting_balances: Balances<CheckedState>,
    token_info: &TokenInfo,
) -> Result<TickStatus, Box<dyn Error>> {
    // 2. Check if Accounting != than latest NewState
    // Accounting.balances != NewState.balances
    // 3. create a NewState
    let new_state = None;

    Ok(TickStatus {
        heartbeat: heartbeat(sentry).await?,
        new_state,
    })
}

async fn _on_new_accounting<A: Adapter + 'static>(
    iface: &SentryApi<A>,
    channel: Channel,
    new_accounting: &AccountingResponse<UncheckedState>,
    token: TokenInfo,
) -> Result<Vec<PropagationResult>, Box<dyn Error>> {
    let state_root_raw = get_state_root_hash(
        channel.id(),
        &Balances::default(),
        token.precision.get(),
    )?;
    let state_root = hex::encode(state_root_raw);

    let signature = iface.adapter.sign(&state_root)?;

    let propagation_results = iface
        .propagate(channel.id(), &[&MessageTypes::NewState(NewState {
            state_root,
            signature,
            balances: new_accounting.balances.clone(),
        })])
        .await;

    Ok(propagation_results)
}
