use std::error::Error;

use primitives::adapter::{Adapter, AdapterErrorKind};
use primitives::{
    validator::{Accounting, MessageTypes, NewState},
    BalancesMap, BigNum,
};

use crate::heartbeat::{heartbeat, HeartbeatStatus};
use crate::sentry_interface::{PropagationResult, SentryApi};
use crate::{get_state_root_hash, producer};

#[derive(Debug)]
pub struct TickStatus<AE: AdapterErrorKind> {
    pub heartbeat: HeartbeatStatus<AE>,
    /// If None, then the conditions for handling a new state haven't been met
    pub new_state: Option<Vec<PropagationResult<AE>>>,
    pub producer_tick: producer::TickStatus<AE>,
}

pub async fn tick<A: Adapter + 'static>(
    iface: &SentryApi<A>,
) -> Result<TickStatus<A::AdapterError>, Box<dyn Error>> {
    let producer_tick = producer::tick(&iface).await?;
    let empty_balances = BalancesMap::default();
    let (balances, new_state) = match &producer_tick {
        producer::TickStatus::Sent { new_accounting, .. } => {
            let new_state = on_new_accounting(&iface, new_accounting).await?;
            (&new_accounting.balances, Some(new_state))
        }
        producer::TickStatus::NoNewEventAggr(balances) => (balances, None),
        producer::TickStatus::EmptyBalances => (&empty_balances, None),
    };

    Ok(TickStatus {
        heartbeat: heartbeat(&iface, &balances).await?,
        new_state,
        producer_tick,
    })
}

async fn on_new_accounting<A: Adapter + 'static>(
    iface: &SentryApi<A>,
    new_accounting: &Accounting,
) -> Result<Vec<PropagationResult<A::AdapterError>>, Box<dyn Error>> {
    let state_root_raw = get_state_root_hash(&iface, &new_accounting.balances)?;
    let state_root = hex::encode(state_root_raw);

    let signature = iface.adapter.sign(&state_root)?;

    let exhausted =
        new_accounting.balances.values().sum::<BigNum>() == iface.channel.deposit_amount;

    let propagation_results = iface
        .propagate(&[&MessageTypes::NewState(NewState {
            state_root,
            signature,
            balances: new_accounting.balances.clone(),
            exhausted,
        })])
        .await;

    Ok(propagation_results)
}
