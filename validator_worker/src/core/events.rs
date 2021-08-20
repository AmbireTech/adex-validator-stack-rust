use num_traits::CheckedSub;

use primitives::sentry::{
    accounting::{Balances, CheckedState},
    AggregateEvents, EventAggregate,
};
use primitives::validator::Accounting;
use primitives::{BalancesMap, BigNum, Channel, DomainError};

//
// TODO #381: AIP#61 Use the new Spender Aggregate and Sum all balances for the new Accounting
// & Temporary allow unnecessary_wraps
#[allow(clippy::unnecessary_wraps)]
pub(crate) fn merge_aggrs(
    accounting: &Accounting,
    aggregates: &[EventAggregate],
    //
    // TODO: AIP#61 Use Campaign and if we should check the total sum of the Balances < campaign.budget
    //
    _channel: &Channel,
) -> Result<Accounting, DomainError> {
    let last_aggregate = [accounting.last_aggregate]
        .iter()
        .chain(aggregates.iter().map(|aggr| &aggr.created))
        .max()
        .unwrap_or(&accounting.last_aggregate)
        .to_owned();

    // Build an intermediary balances representation
    //
    // TODO: AIP#61 Sum all Spender Aggregates and use that for the new Accounting
    //
    let balances = Balances::<CheckedState>::default();

    let new_accounting = Accounting {
        balances,
        last_aggregate,
    };

    Ok(new_accounting)
}

//
// TODO: AIP#61 Check how this should apply for the new Campaigns
//
fn _merge_payouts_into_balances<'a, T: Iterator<Item = &'a AggregateEvents>>(
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

        let new_balance = new_balances.entry(*acc).or_insert_with(|| 0.into());

        *new_balance += to_add;

        remaining = remaining.checked_sub(to_add).ok_or_else(|| {
            DomainError::RuleViolation("remaining must never be negative".to_string())
        })?;
    }

    Ok(new_balances)
}

#[cfg(test)]
mod test {
    use chrono::Utc;

    use primitives::{
        util::tests::prep_db::{
            ADDRESSES, DUMMY_CHANNEL, DUMMY_VALIDATOR_FOLLOWER, DUMMY_VALIDATOR_LEADER,
        },
        Address, Channel, ChannelSpec, ValidatorDesc,
    };

    use super::*;

    #[test]
    #[ignore]
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

        let acc = Accounting {
            balances: BalancesMap::default(),
            last_aggregate: Utc::now(),
        };

        let new_accounting =
            merge_aggrs(&acc, &[gen_ev_aggr(5, &ADDRESSES["publisher"])], &channel)
                .expect("Something went wrong");

        assert_eq!(
            new_accounting.balances[&ADDRESSES["publisher"]],
            148.into(),
            "balances is ok"
        );
    }

    #[test]
    #[ignore]
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

        let acc = Accounting {
            last_aggregate: Utc::now(),
            balances: BalancesMap::default(),
        };

        let new_accounting = merge_aggrs(
            &acc,
            &[gen_ev_aggr(1_001, &ADDRESSES["publisher"])],
            &channel,
        )
        .expect("Something went wrong");

        assert_eq!(
            new_accounting.balances[&ADDRESSES["publisher"]],
            9_702.into(),
            "balances is ok"
        );
        assert_eq!(
            &new_accounting.balances.values().sum::<BigNum>(),
            &channel.deposit_amount,
            "sum(balances) == depositAmount"
        );
    }

    //
    // TODO: AIP#61 Use new Spender Aggregate
    //
    fn gen_ev_aggr(count: u64, recipient: &Address) -> EventAggregate {
        let aggregate_events = AggregateEvents {
            event_counts: Some(vec![(*recipient, count.into())].into_iter().collect()),
            event_payouts: vec![(*recipient, (count * 10).into())]
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
