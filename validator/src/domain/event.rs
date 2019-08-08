use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use adapter::ChannelId;
use chrono::{DateTime, Utc};
use domain::validator::message::Accounting;
use domain::{AdUnit, BalancesMap, BigNum, Channel, DomainError};

#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
#[derive(Serialize, Deserialize)]
pub enum Event {
    #[serde(rename_all = "camelCase")]
    Impression {
        publisher: String,
        ad_unit: AdUnit,
    },
    ImpressionWithCommission {
        earners: Vec<Earner>,
    },
    /// only the creator can send this event
    UpdateImpressionPrice {
        price: BigNum,
    },
    /// only the creator can send this event
    Pay {
        outputs: HashMap<String, BigNum>,
    },
    /// only the creator can send this event
    PauseChannel,
    /// only the creator can send this event
    Close,
}

#[derive(Serialize, Deserialize)]
pub struct Earner {
    #[serde(rename = "publisher")]
    pub address: String,
    pub promilles: u64,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventAggregate {
    pub channel_id: ChannelId,
    pub created: DateTime<Utc>,
    pub events: HashMap<String, AggregateEvents>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AggregateEvents {
    pub event_counts: HashMap<String, BigNum>,
    pub event_payouts: HashMap<String, BigNum>,
}

fn merge_aggregates(
    accounting: &Accounting,
    aggregates: &[EventAggregate],
    channel: &Channel,
) -> Result<(BalancesMap, Accounting), DomainError> {
    let deposit = channel.deposit.clone();
    let balances_before_fees = accounting.pre_fees.clone();
    let balances = balances_before_fees.apply_fees(channel)?;

    let last_event_aggregate = [accounting.last_event_aggregate]
        .iter()
        .chain(aggregates.iter().map(|aggr| aggr.last_event_aggregate))
        .max_by(|lhs, rhs| lhs.cmp(rhs))
        .unwrap_or(aggregates.last_event_aggregate)
        .to_owned();

    let balances_before_fees = aggregates
        .iter()
        .fold(accounting.pre_fees.clone(), |acc, aggr| {
            merge_payouts_into_balances(&acc, aggr.events)
        });

    let new_accounting = Accounting {
        last_event_aggregate,
        pre_fees: balances_before_fees,
        balances: balances.clone(),
    };

    Ok((balances, new_accounting))
}

fn merge_payouts_into_balances(
    balances: &BalancesMap,
    _events: &[Event],
    _deposit: &BigNum,
) -> BalancesMap {
    let _new_balances = balances.clone();

    let _total = balances.sum();

    unimplemented!("Need to do this still")
}
