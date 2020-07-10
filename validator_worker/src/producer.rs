use std::error::Error;

use chrono::{TimeZone, Utc};

use primitives::adapter::{Adapter, AdapterErrorKind};
use primitives::validator::{Accounting, MessageTypes};
use primitives::{BalancesMap, ChannelId};

use crate::core::events::merge_aggrs;
use crate::sentry_interface::{PropagationResult, SentryApi};
use slog::info;

#[derive(Debug)]
pub enum TickStatus<AE: AdapterErrorKind> {
    Sent {
        channel: ChannelId,
        new_accounting: Accounting,
        accounting_propagation: Vec<PropagationResult<AE>>,
        event_counts: usize,
    },
    NoNewEventAggr(BalancesMap),
    EmptyBalances,
}

pub async fn tick<A: Adapter + 'static>(
    iface: &SentryApi<A>,
) -> Result<TickStatus<A::AdapterError>, Box<dyn Error>> {
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
        return Ok(TickStatus::NoNewEventAggr(accounting.balances));
    }

    let new_accounting = merge_aggrs(&accounting, &aggrs.events, &iface.channel)?;

    if new_accounting.balances.is_empty() {
        info!(
            iface.logger,
            "channel {}: empty Accounting balances, skipping propagation", iface.channel.id
        );

        Ok(TickStatus::EmptyBalances)
    } else {
        info!(
            iface.logger,
            "channel {}: processed {} event aggregates",
            iface.channel.id,
            aggrs.events.len()
        );

        let message_types = MessageTypes::Accounting(new_accounting.clone());

        Ok(TickStatus::Sent {
            channel: iface.channel.id,
            accounting_propagation: iface.propagate(&[&message_types]).await,
            new_accounting,
            event_counts: aggrs.events.len(),
        })
    }
}
