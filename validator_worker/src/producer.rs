use std::error::Error;

use chrono::{TimeZone, Utc};

use primitives::adapter::{Adapter, AdapterErrorKind};
use primitives::validator::{Accounting, MessageTypes};
use primitives::{sentry::accounting::{Balances, CheckedState}, ChannelId};

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
    NoNewEventAggr(Balances<CheckedState>),
    EmptyBalances,
}

pub async fn tick<A: Adapter + 'static>(
    iface: &SentryApi<A>,
) -> Result<TickStatus<A::AdapterError>, Box<dyn Error>> {
    let validator_msg_resp = iface.get_our_latest_msg(&["Accounting"]).await?;

    let accounting = match validator_msg_resp {
        Some(MessageTypes::Accounting(accounting)) => accounting,
        _ => Accounting {
            last_aggregate: Utc.timestamp(0, 0),
            balances: Default::default(),
        },
    };

    //
    // TODO #381: AIP#61 Merge all Spender Aggregates and create a new Accounting
    //

    let aggrs = iface
        .get_event_aggregates(accounting.last_aggregate)
        .await?;

    if aggrs.events.is_empty() {
        return Ok(TickStatus::NoNewEventAggr(accounting.balances));
    }

    //
    // TODO: AIP#61 Merge all Spender Aggregates when it's implemented
    //
    let new_accounting = merge_aggrs(&accounting, &aggrs.events, &iface.channel)?;

    if new_accounting.balances.earners.is_empty() || new_accounting.balances.spenders.is_empty() {
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
