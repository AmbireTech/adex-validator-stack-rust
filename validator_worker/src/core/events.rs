use num_traits::CheckedSub;

use primitives::sentry::{AggregateEvents, EventAggregate};
use primitives::validator::Accounting;
use primitives::{BalancesMap, BigNum, Channel, DomainError};

use crate::core::fees::get_balances_after_fees_tree;

pub(crate) fn merge_aggrs(
    accounting: &Accounting,
    aggregates: &[EventAggregate],
    channel: &Channel,
) -> Result<Accounting, DomainError> {
    let deposit = channel.deposit_amount.clone();

    let last_event_aggregate = [accounting.last_event_aggregate]
        .iter()
        .chain(aggregates.iter().map(|aggr| &aggr.created))
        .max()
        .unwrap_or(&accounting.last_event_aggregate)
        .to_owned();

    // Build an intermediary balances representation
    let mut balances_before_fees = accounting.balances_before_fees.clone();

    // Merge in all the aggrs
    for aggr in aggregates {
        balances_before_fees =
            merge_payouts_into_balances(&balances_before_fees, aggr.events.values(), &deposit)?
    }

    // apply fees
    let balances = get_balances_after_fees_tree(&balances_before_fees, &channel)?;

    let new_accounting = Accounting {
        last_event_aggregate,
        balances_before_fees,
        balances,
    };

    Ok(new_accounting)
}

fn merge_payouts_into_balances<'a, T: Iterator<Item = &'a AggregateEvents>>(
    balances: &BalancesMap,
    events: T,
    deposit: &BigNum,
) -> Result<BalancesMap, DomainError> {
    let mut new_balances = balances.clone();

    let total = balances.values().sum();
    let mut remaining = deposit.checked_sub(&total).ok_or_else(|| {
        DomainError::RuleViolation("remaining starts negative: total>depositAmount".to_string())
    })?;

    let all_payouts = events.map(|aggr_ev| aggr_ev.event_payouts.iter()).flatten();

    for (acc, payout) in all_payouts {
        let to_add = payout.min(&remaining);

        let new_balance = new_balances
            .entry(acc.to_owned())
            .or_insert_with(|| 0.into());

        *new_balance += &to_add;

        remaining = remaining.checked_sub(&to_add).ok_or_else(|| {
            DomainError::RuleViolation("remaining must never be negative".to_string())
        })?;
    }

    Ok(new_balances)
}

#[cfg(test)]
mod test {
    use chrono::Utc;

    use primitives::util::tests::prep_db::{
        DUMMY_CHANNEL, DUMMY_VALIDATOR_FOLLOWER, DUMMY_VALIDATOR_LEADER, IDS,
    };
    use primitives::{Channel, ChannelSpec, ValidatorDesc, ValidatorId};

    use super::*;

    #[test]
    fn should_merge_event_aggrs_and_apply_fees() {
        // fees: 100
        // deposit: 10 000
        let leader = ValidatorDesc {
            fee: 50.into(),
            ..DUMMY_VALIDATOR_LEADER.clone()
        };
        let follower = ValidatorDesc {
            fee: 50.into(),
            ..DUMMY_VALIDATOR_FOLLOWER.clone()
        };

        let mut channel = Channel {
            deposit_amount: 10_000.into(),
            ..DUMMY_CHANNEL.clone()
        };
        channel.spec.validators = (leader, follower).into();

        let balances_before_fees: BalancesMap = vec![
            (IDS["publisher"].clone(), 100.into()),
            (IDS["publisher2"].clone(), 200.into()),
        ]
        .into_iter()
        .collect();

        let acc = Accounting {
            last_event_aggregate: Utc::now(),
            balances_before_fees,
            balances: BalancesMap::default(),
        };

        let new_accounting = merge_aggrs(&acc, &[gen_ev_aggr(5, &IDS["publisher"])], &channel)
            .expect("Something went wrong");

        assert_eq!(
            new_accounting.balances_before_fees[&IDS["publisher"]],
            150.into(),
            "balance of recipient incremented accordingly"
        );
        assert_eq!(
            new_accounting.balances[&IDS["publisher"]],
            148.into(),
            "balanceAfterFees is ok"
        );
    }

    #[test]
    fn should_never_allow_exceeding_the_deposit() {
        let leader = ValidatorDesc {
            fee: 50.into(),
            ..DUMMY_VALIDATOR_LEADER.clone()
        };
        let follower = ValidatorDesc {
            fee: 50.into(),
            ..DUMMY_VALIDATOR_FOLLOWER.clone()
        };

        let spec = ChannelSpec {
            validators: (leader, follower).into(),
            ..DUMMY_CHANNEL.spec.clone()
        };
        let channel = Channel {
            deposit_amount: 10_000.into(),
            spec,
            ..DUMMY_CHANNEL.clone()
        };

        let balances_before_fees: BalancesMap = vec![
            (IDS["publisher"].clone(), 100.into()),
            (IDS["publisher2"].clone(), 200.into()),
        ]
        .into_iter()
        .collect();

        let acc = Accounting {
            last_event_aggregate: Utc::now(),
            balances_before_fees,
            balances: BalancesMap::default(),
        };

        let new_accounting = merge_aggrs(&acc, &[gen_ev_aggr(1_001, &IDS["publisher"])], &channel)
            .expect("Something went wrong");

        assert_eq!(
            new_accounting.balances_before_fees[&IDS["publisher"]],
            9_800.into(),
            "balance of recipient incremented accordingly"
        );
        assert_eq!(
            new_accounting.balances_before_fees[&IDS["publisher2"]],
            200.into(),
            "balances of non-recipient remains the same"
        );
        assert_eq!(
            new_accounting.balances[&IDS["publisher"]],
            9_702.into(),
            "balanceAfterFees is ok"
        );
        assert_eq!(
            &new_accounting.balances_before_fees.values().sum::<BigNum>(),
            &channel.deposit_amount,
            "sum(balancesBeforeFees) == depositAmount"
        );
        assert_eq!(
            &new_accounting.balances.values().sum::<BigNum>(),
            &channel.deposit_amount,
            "sum(balances) == depositAmount"
        );
    }

    fn gen_ev_aggr(count: u64, recipient: &ValidatorId) -> EventAggregate {
        let aggregate_events = AggregateEvents {
            event_counts: Some(
                vec![(recipient.clone(), count.into())]
                    .into_iter()
                    .collect(),
            ),
            event_payouts: vec![(recipient.clone(), (count * 10).into())]
                .into_iter()
                .collect(),
        };

        EventAggregate {
            channel_id: DUMMY_CHANNEL.id.to_owned(),
            created: Utc::now(),
            events: vec![("IMPRESSION".to_string(), aggregate_events)]
                .into_iter()
                .collect(),
        }
    }
}
