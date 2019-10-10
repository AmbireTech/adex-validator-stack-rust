use std::error::Error;

use chrono::{TimeZone, Utc};

use primitives::adapter::Adapter;
use primitives::validator::{Accounting, MessageTypes};
use primitives::BalancesMap;

use crate::core::events::merge_aggrs;
use crate::sentry_interface::SentryApi;

pub type Result = std::result::Result<(BalancesMap, Option<Accounting>), Box<dyn Error>>;

pub async fn tick<A: Adapter + 'static>(iface: &SentryApi<A>) -> Result {
    let validator_msg_resp = iface.get_our_latest_msg("Accounting".to_owned()).await?;

    let accounting = validator_msg_resp
        .msg
        .get(0)
        .and_then(|accounting| match accounting {
            MessageTypes::Accounting(accounting) => Some(accounting.to_owned()),
            _ => None,
        })
        .unwrap_or_else(|| Accounting {
            last_event_aggregate: Utc.timestamp(0, 0),
            balances_before_fees: Default::default(),
            balances: Default::default(),
        });

    let aggrs = iface
        .get_event_aggregates(accounting.last_event_aggregate)
        .await?;

    if !aggrs.events.is_empty() {
        // TODO: Log the merge
        let (balances, new_accounting) = merge_aggrs(&accounting, &aggrs.events, &iface.channel)?;

        let message_types = MessageTypes::Accounting(new_accounting.clone());
        iface.propagate(&[&message_types]);

        Ok((balances, Some(new_accounting)))
    } else {
        Ok((accounting.balances.clone(), None))
    }
}
