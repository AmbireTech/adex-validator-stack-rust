use std::error::Error;

use chrono::{TimeZone, Utc};

use primitives::adapter::Adapter;
use primitives::validator::{Accounting, MessageTypes};
use primitives::BalancesMap;

use crate::core::events::merge_aggrs;
use crate::sentry_interface::SentryApi;
use slog::info;

pub type Result = std::result::Result<(BalancesMap, Option<Accounting>), Box<dyn Error>>;

pub async fn tick<A: Adapter + 'static>(iface: &SentryApi<A>) -> Result {
    let validator_msg_resp = iface.get_our_latest_msg(&["Accounting"]).await?;

    let accounting = match validator_msg_resp {
        Some(MessageTypes::Accounting(accounting)) => accounting,
        _ => Accounting {
            last_event_aggregate: Utc.timestamp(0, 0),
            balances_before_fees: Default::default(),
            balances: Default::default(),
        },
    };

    let aggrs = iface
        .get_event_aggregates(accounting.last_event_aggregate)
        .await?;

    if aggrs.events.is_empty() {
        return Ok((accounting.balances, None));
    }

    let (balances, new_accounting) = merge_aggrs(&accounting, &aggrs.events, &iface.channel)?;

    if new_accounting.balances.is_empty() {
        info!(
            iface.logger,
            "channel {}: empty Accounting balances, skipping propagation", iface.channel.id
        );

        Ok((balances, None))
    } else {
        info!(
            iface.logger,
            "channel {}: processed {} event aggregates",
            iface.channel.id,
            aggrs.events.len()
        );

        let message_types = MessageTypes::Accounting(new_accounting.clone());
        iface.propagate(&[&message_types]).await;

        Ok((balances, Some(new_accounting)))
    }
}
