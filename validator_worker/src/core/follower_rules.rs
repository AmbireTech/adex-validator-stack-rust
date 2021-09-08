use primitives::{balances::CheckedState, Balances, UnifiedNum};

pub fn is_valid_transition(
    // spendable: &HashMap<Address, Spendable>,
    prev: &Balances<CheckedState>,
    next: &Balances<CheckedState>,
) -> Option<bool> {
    let (earners_prev, spenders_prev) = prev.sum()?;

    let (earners_next, spenders_next) = next.sum()?;

    let prev_checks = {
        let prev_spenders = prev
            .spenders
            .iter()
            .all(|(acc, bal)| match next.spenders.get(acc) {
                Some(next_bal) => next_bal >= bal,
                None => false,
            });

        let prev_earners = prev
            .earners
            .iter()
            .all(|(acc, bal)| match next.earners.get(acc) {
                Some(next_bal) => next_bal >= bal,
                None => false,
            });

        prev_spenders && prev_earners
    };

    // no need to check if there are negative balances as we don't allow them using UnifiedNum
    Some(earners_next >= earners_prev && spenders_next >= spenders_prev && prev_checks)
}

pub fn get_health(
    // spendable: &HashMap<Address, Spender>,
    our: &Balances<CheckedState>,
    approved: &Balances<CheckedState>,
) -> Option<u64> {
    let zero = UnifiedNum::default();

    // let spenders_sum = spendable.values().map(|spender| spender.total_deposited).sum::<Option<_>>()?;

    let (sum_our_earners, sum_our_spenders) = our.sum()?;

    let (sum_approved_mins_earners, sum_approved_mins_spenders) = (
        our.earners
            .iter()
            .map(|(acc, val)| val.min(approved.earners.get(acc).unwrap_or(&zero)))
            .sum::<Option<_>>()?,
        our.spenders
            .iter()
            .map(|(acc, val)| val.min(approved.spenders.get(acc).unwrap_or(&zero)))
            .sum::<Option<_>>()?,
    );

    if sum_approved_mins_earners >= sum_our_earners
        && sum_approved_mins_spenders >= sum_our_spenders
    {
        return Some(1_000);
    }

    let diff_earners = sum_our_earners - sum_approved_mins_earners;
    let diff_spenders = sum_our_spenders - sum_approved_mins_spenders;

    // TODO: What should the health check include?
    // v4 OLD - let health_penalty = diff * &UnifiedNum::from(1_000) / &channel.deposit_amount;
    // TODO: Check if taking the difference between the two sums (earners & spenders) is good enough as a health check!

    // Because Accounting should always have >= values than the NewState ones
    let health_penalty = diff_earners * UnifiedNum::from(1_000) / diff_spenders;

    Some(1_000 - health_penalty.to_u64())
}

#[cfg(test)]
mod test {
    use primitives::util::tests::prep_db::ADDRESSES;

    use super::*;

    const HEALTH_THRESHOLD: u64 = 950;

    #[test]
    fn is_valid_transition_empty_to_empty() {
        assert!(
            is_valid_transition(&Balances::default(), &Balances::default()).expect("No overflow"),
            "is valid transition"
        )
    }

    #[test]
    fn is_valid_transition_a_valid_transition() {
        let mut next: Balances<CheckedState> = Balances::default();
        next.spend(ADDRESSES["creator"], ADDRESSES["publisher"], 100.into())
            .expect("Should not overflow");

        assert!(
            is_valid_transition(&Balances::default(), &next).expect("No overflow"),
            "is valid transition"
        )
    }

    #[test]
    fn is_valid_transition_single_value_is_lower() {
        let mut prev: Balances<CheckedState> = Balances::default();
        prev.spend(ADDRESSES["creator"], ADDRESSES["publisher"], 55.into())
            .expect("Should not overflow");

        let mut next: Balances<CheckedState> = Balances::default();
        next.spend(ADDRESSES["user"], ADDRESSES["publisher"], 54.into())
            .expect("Should not overflow");

        assert!(
            !is_valid_transition(&prev, &next).expect("No overflow"),
            "not a valid transition"
        );
    }

    #[test]
    fn is_valid_transition_a_value_is_lower_but_overall_sum_is_higher() {
        let mut prev: Balances<CheckedState> = Balances::default();
        prev.spend(ADDRESSES["user"], ADDRESSES["publisher"], 55.into())
            .expect("Should not overflow");

        let mut next: Balances<CheckedState> = Balances::default();
        next.spend(ADDRESSES["user"], ADDRESSES["publisher"], 54.into())
            .expect("Should not overflow");
        next.spend(ADDRESSES["user"], ADDRESSES["publisher2"], 3.into())
            .expect("Should not overflow");

        assert!(
            !is_valid_transition(&prev, &next).expect("No overflow"),
            "not a valid transition"
        );
    }

    #[test]
    fn is_valid_transition_overall_sum_is_lower() {
        let mut prev: Balances<CheckedState> = Balances::default();
        prev.spend(ADDRESSES["user"], ADDRESSES["publisher"], 54.into())
            .expect("Should not overflow");
        prev.spend(ADDRESSES["user"], ADDRESSES["publisher2"], 3.into())
            .expect("Should not overflow");

        let mut next: Balances<CheckedState> = Balances::default();
        next.spend(ADDRESSES["user"], ADDRESSES["publisher"], 54.into())
            .expect("Should not overflow");

        assert!(
            !is_valid_transition(&prev, &next).expect("No overflow"),
            "not a valid transition"
        );
    }

    #[test]
    fn is_valid_transition_overall_sum_is_the_same_but_we_remove_an_entry() {
        let mut prev: Balances<CheckedState> = Balances::default();
        prev.spend(ADDRESSES["user"], ADDRESSES["publisher"], 54.into())
            .expect("Should not overflow");
        prev.spend(ADDRESSES["user"], ADDRESSES["publisher2"], 3.into())
            .expect("Should not overflow");

        let mut next: Balances<CheckedState> = Balances::default();
        next.spend(ADDRESSES["user"], ADDRESSES["publisher"], 57.into())
            .expect("Should not overflow");

        assert!(
            !is_valid_transition(&prev, &next).expect("No overflow"),
            "not a valid transition"
        );
    }

    #[test]
    fn get_health_the_approved_balance_tree_gte_our_accounting_is_healthy() {
        let mut our: Balances<CheckedState> = Balances::default();
        our.spend(ADDRESSES["user"], ADDRESSES["publisher"], 50.into())
            .expect("Should not overflow");

        assert!(get_health(&our, &our).expect("No overflow") >= HEALTH_THRESHOLD);

        let mut approved: Balances<CheckedState> = Balances::default();
        approved
            .spend(ADDRESSES["user"], ADDRESSES["publisher"], 60.into())
            .expect("Should not overflow");

        assert!(get_health(&our, &approved).expect("No overflow") >= HEALTH_THRESHOLD);
    }

    // #[test]
    // fn get_health_the_approved_balance_tree_is_positive_our_accounting_is_0_and_it_is_healthy() {
    //     let approved = vec![(ADDRESSES["publisher"], 50.into())]
    //         .into_iter()
    //         .collect();

    //     assert!(
    //         get_health(&get_dummy_channel(50), &BalancesMap::default(), &approved)
    //             >= HEALTH_THRESHOLD
    //     );
    // }

    // #[test]
    // fn get_health_the_approved_balance_tree_has_less_but_within_margin_it_is_healthy() {
    //     let channel = get_dummy_channel(80);

    //     assert!(
    //         get_health(
    //             &channel,
    //             &vec![(ADDRESSES["publisher"], 80.into())]
    //                 .into_iter()
    //                 .collect(),
    //             &vec![(ADDRESSES["publisher"], 79.into())]
    //                 .into_iter()
    //                 .collect()
    //         ) >= HEALTH_THRESHOLD
    //     );

    //     assert!(
    //         get_health(
    //             &channel,
    //             &vec![(ADDRESSES["publisher"], 2.into())]
    //                 .into_iter()
    //                 .collect(),
    //             &vec![(ADDRESSES["publisher"], 1.into())]
    //                 .into_iter()
    //                 .collect()
    //         ) >= HEALTH_THRESHOLD
    //     );
    // }

    // #[test]
    // fn get_health_the_approved_balance_tree_has_less_it_is_unhealthy() {
    //     let channel = get_dummy_channel(80);

    //     assert!(
    //         get_health(
    //             &channel,
    //             &vec![(ADDRESSES["publisher"], 80.into())]
    //                 .into_iter()
    //                 .collect(),
    //             &vec![(ADDRESSES["publisher"], 70.into())]
    //                 .into_iter()
    //                 .collect()
    //         ) < HEALTH_THRESHOLD
    //     );
    // }

    // #[test]
    // fn get_health_they_have_the_same_sum_but_different_entities_are_earning() {
    //     let channel = get_dummy_channel(80);

    //     assert!(
    //         get_health(
    //             &channel,
    //             &vec![(ADDRESSES["publisher"], 80.into())]
    //                 .into_iter()
    //                 .collect(),
    //             &vec![(ADDRESSES["publisher2"], 80.into())]
    //                 .into_iter()
    //                 .collect()
    //         ) < HEALTH_THRESHOLD
    //     );

    //     assert!(
    //         get_health(
    //             &channel,
    //             &vec![(ADDRESSES["publisher"], 80.into())]
    //                 .into_iter()
    //                 .collect(),
    //             &vec![
    //                 (ADDRESSES["publisher2"], 40.into()),
    //                 (ADDRESSES["publisher"], 40.into())
    //             ]
    //             .into_iter()
    //             .collect()
    //         ) < HEALTH_THRESHOLD
    //     );

    //     assert!(
    //         get_health(
    //             &channel,
    //             &vec![(ADDRESSES["publisher"], 80.into())]
    //                 .into_iter()
    //                 .collect(),
    //             &vec![
    //                 (ADDRESSES["publisher2"], 20.into()),
    //                 (ADDRESSES["publisher"], 60.into())
    //             ]
    //             .into_iter()
    //             .collect()
    //         ) < HEALTH_THRESHOLD
    //     );

    //     assert!(
    //         get_health(
    //             &channel,
    //             &vec![(ADDRESSES["publisher"], 80.into())]
    //                 .into_iter()
    //                 .collect(),
    //             &vec![
    //                 (ADDRESSES["publisher2"], 2.into()),
    //                 (ADDRESSES["publisher"], 78.into())
    //             ]
    //             .into_iter()
    //             .collect()
    //         ) >= HEALTH_THRESHOLD
    //     );

    //     assert!(
    //         get_health(
    //             &channel,
    //             &vec![
    //                 (ADDRESSES["publisher"], 100.into()),
    //                 (ADDRESSES["publisher2"], 1.into())
    //             ]
    //             .into_iter()
    //             .collect(),
    //             &vec![(ADDRESSES["publisher"], 100.into())]
    //                 .into_iter()
    //                 .collect()
    //         ) >= HEALTH_THRESHOLD
    //     );
    // }
}
