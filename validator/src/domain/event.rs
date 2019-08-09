use std::collections::HashMap;

use chrono::{DateTime, Utc};
use num_traits::CheckedSub;
use serde::{Deserialize, Serialize};

use domain::balances_map::get_balances_after_fees_tree;
use domain::validator::message::Accounting;
use domain::{AdUnit, BalancesMap, BigNum, Channel, ChannelId, DomainError};

#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
#[derive(Serialize, Deserialize)]
pub enum Event {
    #[serde(rename_all = "camelCase")]
    Impression {
        publisher: String,
        // clippy warning for big size difference, because of field
        ad_unit: Box<AdUnit>,
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

#[allow(dead_code)]
fn merge_aggrs(
    accounting: &Accounting,
    aggregates: &[EventAggregate],
    channel: &Channel,
) -> Result<(BalancesMap, Accounting), DomainError> {
    let deposit = channel.deposit_amount.clone();

    let last_event_aggregate = [accounting.last_event_aggregate]
        .iter()
        .chain(aggregates.iter().map(|aggr| &aggr.created))
        .max_by(|lhs, rhs| lhs.cmp(rhs))
        .unwrap_or(&accounting.last_event_aggregate)
        .to_owned();

    // Build an intermediary balances representation
    let mut balances_before_fees = accounting.pre_fees.clone();

    // Merge in all the aggrs
    for aggr in aggregates {
        balances_before_fees =
            merge_payouts_into_balances(&balances_before_fees, aggr.events.values(), &deposit)?
    }

    // apply fees
    let balances = get_balances_after_fees_tree(&balances_before_fees, &channel)?;

    let new_accounting = Accounting {
        last_event_aggregate,
        pre_fees: balances_before_fees,
        balances: balances.clone(),
    };

    Ok((balances, new_accounting))
}

fn merge_payouts_into_balances<'a, T: Iterator<Item = &'a AggregateEvents>>(
    balances: &BalancesMap,
    events: T,
    deposit: &BigNum,
) -> Result<BalancesMap, DomainError> {
    let mut new_balances = balances.clone();

    let total = balances.values().sum();
    let mut remaining = deposit
        .checked_sub(&total)
        .ok_or(DomainError::RuleViolation(
            "remaining starts negative: total>depositAmount".to_string(),
        ))?;

    let all_payouts = events.map(|aggr_ev| aggr_ev.event_payouts.iter()).flatten();

    for (acc, payout) in all_payouts {
        let to_add = payout.min(&remaining);

        let new_balance = new_balances
            .entry(acc.to_owned())
            .or_insert_with(|| 0.into());

        *new_balance += &to_add;

        remaining = remaining
            .checked_sub(&to_add)
            .ok_or(DomainError::RuleViolation(
                "remaining must never be negative".to_string(),
            ))?;
    }

    Ok(new_balances)
}

#[cfg(test)]
mod test {
    use super::*;
    use domain::channel::fixtures::{get_channel, get_channel_spec, ValidatorsOption};
    use domain::fixtures::get_channel_id;
    use domain::validator::fixtures::get_validator;

    #[test]
    fn should_merge_event_aggrs_and_apply_fees() {
        // fees: 100
        // deposit: 10000
        let leader = get_validator("one", Some(50.into()));
        let follower = get_validator("two", Some(50.into()));

        let spec = get_channel_spec(ValidatorsOption::Pair { leader, follower });
        let mut channel = get_channel("channel", &None, Some(spec));
        channel.deposit_amount = 10_000.into();

        let balances_before_fees: BalancesMap =
            vec![("a".to_string(), 100.into()), ("b".to_string(), 200.into())]
                .into_iter()
                .collect();

        let acc = Accounting {
            last_event_aggregate: Utc::now(),
            pre_fees: balances_before_fees,
            balances: BalancesMap::default(),
        };

        let (balances, new_accounting) =
            merge_aggrs(&acc, &[gen_ev_aggr(5, "a")], &channel).expect("Something went wrong");

        assert_eq!(balances, new_accounting.balances, "balances is the same");
        assert_eq!(
            new_accounting.pre_fees["a"],
            150.into(),
            "balance of recipient incremented accordingly"
        );
        assert_eq!(
            new_accounting.balances["a"],
            148.into(),
            "balanceAfterFees is ok"
        );
    }

    #[test]
    fn should_never_allow_exceeding_the_deposit() {
        let leader = get_validator("one", Some(50.into()));
        let follower = get_validator("two", Some(50.into()));

        let spec = get_channel_spec(ValidatorsOption::Pair { leader, follower });
        let mut channel = get_channel("channel", &None, Some(spec));
        channel.deposit_amount = 10_000.into();

        let balances_before_fees: BalancesMap =
            vec![("a".to_string(), 100.into()), ("b".to_string(), 200.into())]
                .into_iter()
                .collect();

        let acc = Accounting {
            last_event_aggregate: Utc::now(),
            pre_fees: balances_before_fees,
            balances: BalancesMap::default(),
        };

        let (balances, new_accounting) =
            merge_aggrs(&acc, &[gen_ev_aggr(1_001, "a")], &channel).expect("Something went wrong");

        assert_eq!(balances, new_accounting.balances, "balances is the same");
        assert_eq!(
            new_accounting.pre_fees["a"],
            9_800.into(),
            "balance of recipient incremented accordingly"
        );
        assert_eq!(
            new_accounting.pre_fees["b"],
            200.into(),
            "balances of non-recipient remains the same"
        );
        assert_eq!(
            new_accounting.balances["a"],
            9_702.into(),
            "balanceAfterFees is ok"
        );
        assert_eq!(
            &new_accounting.pre_fees.values().sum::<BigNum>(),
            &channel.deposit_amount,
            "sum(balancesBeforeFees) == depositAmount"
        );
        assert_eq!(
            &new_accounting.balances.values().sum::<BigNum>(),
            &channel.deposit_amount,
            "sum(balances) == depositAmount"
        );
    }

    fn gen_ev_aggr(count: u64, recipient: &str) -> EventAggregate {
        let aggregate_events = AggregateEvents {
            event_counts: vec![(recipient.to_string(), count.into())]
                .into_iter()
                .collect(),
            event_payouts: vec![(recipient.to_string(), (count * 10).into())]
                .into_iter()
                .collect(),
        };

        EventAggregate {
            channel_id: get_channel_id("one"),
            created: Utc::now(),
            events: vec![("IMPRESSION".to_string(), aggregate_events)]
                .into_iter()
                .collect(),
        }
    }
}
