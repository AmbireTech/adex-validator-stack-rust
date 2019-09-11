use std::error::Error;

use futures::future::FutureExt;

use primitives::adapter::Adapter;
use primitives::{Channel, BalancesMap};
use primitives::validator::{Validator, ValidatorFuture, Accounting, MessageTypes, NewState};
use crate::{producer, get_state_root_hash};

use crate::sentry_interface::SentryApi;

pub async fn tick<A: Adapter + 'static>(iface: &SentryApi<A>) -> Result<(), Box<dyn Error>> {
    let (balances, new_accounting) = await!(producer::tick(&iface))?;

    if let Some(new_accounting) = new_accounting {
        await!(on_new_accounting(&iface, (&balances, &new_accounting)))?;
    }

    Ok(())
}

fn on_new_accounting<A: Adapter + 'static>(iface: &SentryApi<A>, (balances, new_accounting): (&BalancesMap, &Accounting)) -> Result<(), Box<dyn Error>> {
    let state_root_raw = get_state_root_hash(&iface, &balances);

    let signature = iface.adapter.sign(&state_root_raw)?;

    // TODO: Get Hex state root
    let state_root = state_root_raw.clone();

    iface.propagate(&[&MessageTypes::NewState(NewState {
        state_root,
        signature,
        balances: new_accounting.balances.clone()
    })])?;

    Ok(())
}