use std::error::Error;

use primitives::BalancesMap;
use primitives::adapter::Adapter;
use primitives::validator::{Accounting, MessageTypes, NewState};

use crate::{get_state_root_hash, producer};
use crate::sentry_interface::SentryApi;

pub async fn tick<A: Adapter + 'static>(iface: &SentryApi<A>) -> Result<(), Box<dyn Error>> {
    let (balances, new_accounting) = await!(producer::tick(&iface))?;

    if let Some(new_accounting) = new_accounting {
        on_new_accounting(&iface, (&balances, &new_accounting))?;
    }

    Ok(())
}

fn on_new_accounting<A: Adapter + 'static>(
    iface: &SentryApi<A>,
    (balances, new_accounting): (&BalancesMap, &Accounting),
) -> Result<(), Box<dyn Error>> {
    let state_root_raw = get_state_root_hash(&iface, &balances)?;
    let state_root = hex::encode(state_root_raw);

    let signature = iface.adapter.sign(&hex::encode(&state_root))?;

    iface.propagate(&[&MessageTypes::NewState(NewState {
        state_root,
        signature,
        balances: new_accounting.balances.clone(),
    })]);

    Ok(())
}
