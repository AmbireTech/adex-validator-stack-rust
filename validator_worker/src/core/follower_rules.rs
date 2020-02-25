use primitives::{BalancesMap, BigNum, Channel};

pub fn is_valid_transition(channel: &Channel, prev: &BalancesMap, next: &BalancesMap) -> bool {
    let sum_prev: BigNum = prev.values().sum();
    let sum_next: BigNum = next.values().sum();

    let deposit = channel.deposit_amount.clone();

    let prev_checks = prev.iter().all(|(acc, bal)| match next.get(acc) {
        Some(next_bal) => next_bal >= bal,
        None => false,
    });

    // no need to check if there are negative balances as we don't allow them using BigUint
    sum_next >= sum_prev && sum_next <= deposit && prev_checks
}

pub fn get_health(channel: &Channel, our: &BalancesMap, approved: &BalancesMap) -> u64 {
    let sum_our: BigNum = our.values().sum();

    let zero = BigNum::from(0);
    let sum_approved_mins = our
        .iter()
        .map(|(acc, val)| val.min(approved.get(acc).unwrap_or(&zero)))
        .sum();

    if sum_approved_mins >= sum_our {
        return 1_000;
    }

    let diff = sum_our - sum_approved_mins;
    let health_penalty = diff * &BigNum::from(1_000) / &channel.deposit_amount;
    1_000 - health_penalty.to_u64().unwrap_or(1_000)
}

#[cfg(test)]
mod test {
    use primitives::util::tests::prep_db::{DUMMY_CHANNEL, IDS};

    use super::*;

    const HEALTH_THRESHOLD: u64 = 950;

    fn get_dummy_channel<T: Into<BigNum>>(deposit: T) -> Channel {
        Channel {
            deposit_amount: deposit.into(),
            ..DUMMY_CHANNEL.clone()
        }
    }

    #[test]
    fn is_valid_transition_empty_to_empty() {
        assert!(
            is_valid_transition(
                &get_dummy_channel(100),
                &BalancesMap::default(),
                &BalancesMap::default(),
            ),
            "is valid transition"
        )
    }

    #[test]
    fn is_valid_transition_a_valid_transition() {
        let next = vec![(IDS["publisher"].clone(), 100.into())]
            .into_iter()
            .collect();

        assert!(
            is_valid_transition(&get_dummy_channel(100), &BalancesMap::default(), &next,),
            "is valid transition"
        )
    }

    #[test]
    fn is_valid_transition_more_funds_than_dummy_channel() {
        let next = vec![
            (IDS["publisher"].clone(), 51.into()),
            (IDS["publisher2"].clone(), 50.into()),
        ]
        .into_iter()
        .collect();

        assert!(
            !is_valid_transition(&get_dummy_channel(100), &BalancesMap::default(), &next),
            "not a valid transition"
        );
    }

    #[test]
    fn is_valid_transition_single_value_is_lower() {
        let prev = vec![(IDS["publisher"].clone(), 55.into())]
            .into_iter()
            .collect();

        let next = vec![(IDS["publisher"].clone(), 54.into())]
            .into_iter()
            .collect();

        assert!(
            !is_valid_transition(&get_dummy_channel(100), &prev, &next),
            "not a valid transition"
        );
    }

    #[test]
    fn is_valid_transition_a_value_is_lower_but_overall_sum_is_higher() {
        let prev = vec![(IDS["publisher"].clone(), 55.into())]
            .into_iter()
            .collect();

        let next = vec![
            (IDS["publisher"].clone(), 54.into()),
            (IDS["publisher2"].clone(), 3.into()),
        ]
        .into_iter()
        .collect();

        assert!(
            !is_valid_transition(&get_dummy_channel(100), &prev, &next),
            "not a valid transition"
        );
    }

    #[test]
    fn is_valid_transition_overall_sum_is_lower() {
        let prev = vec![
            (IDS["publisher"].clone(), 54.into()),
            (IDS["publisher2"].clone(), 3.into()),
        ]
        .into_iter()
        .collect();

        let next = vec![(IDS["publisher"].clone(), 54.into())]
            .into_iter()
            .collect();

        assert!(
            !is_valid_transition(&get_dummy_channel(100), &prev, &next),
            "not a valid transition"
        );
    }

    #[test]
    fn is_valid_transition_overall_sum_is_the_same_but_we_remove_an_entry() {
        let prev = vec![
            (IDS["publisher"].clone(), 54.into()),
            (IDS["publisher2"].clone(), 3.into()),
        ]
        .into_iter()
        .collect();

        let next = vec![(IDS["publisher"].clone(), 57.into())]
            .into_iter()
            .collect();

        assert!(
            !is_valid_transition(&get_dummy_channel(100), &prev, &next),
            "not a valid transition"
        );
    }

    #[test]
    fn is_valid_transition_transition_to_a_state_with_a_negative_number() {
        let prev = vec![
            (IDS["publisher"].clone(), 54.into()),
            (IDS["publisher2"].clone(), 3.into()),
        ]
        .into_iter()
        .collect();

        let next = vec![(IDS["publisher"].clone(), 57.into())]
            .into_iter()
            .collect();

        assert!(
            !is_valid_transition(&get_dummy_channel(100), &prev, &next),
            "not a valid transition"
        );
    }

    #[test]
    fn get_health_the_approved_balance_tree_gte_our_accounting_is_healthy() {
        let channel = get_dummy_channel(50);
        let our = vec![(IDS["publisher"].clone(), 50.into())]
            .into_iter()
            .collect();
        assert!(get_health(&channel, &our, &our) >= HEALTH_THRESHOLD);

        assert!(
            get_health(
                &channel,
                &our,
                &vec![(IDS["publisher"].clone(), 60.into())]
                    .into_iter()
                    .collect()
            ) >= HEALTH_THRESHOLD
        );
    }

    #[test]
    fn get_health_the_approved_balance_tree_is_positive_our_accounting_is_0_and_it_is_healthy() {
        let approved = vec![(IDS["publisher"].clone(), 50.into())]
            .into_iter()
            .collect();

        assert!(
            get_health(&get_dummy_channel(50), &BalancesMap::default(), &approved)
                >= HEALTH_THRESHOLD
        );
    }

    #[test]
    fn get_health_the_approved_balance_tree_has_less_but_within_margin_it_is_healthy() {
        let channel = get_dummy_channel(80);

        assert!(
            get_health(
                &channel,
                &vec![(IDS["publisher"].clone(), 80.into())]
                    .into_iter()
                    .collect(),
                &vec![(IDS["publisher"].clone(), 79.into())]
                    .into_iter()
                    .collect()
            ) >= HEALTH_THRESHOLD
        );

        assert!(
            get_health(
                &channel,
                &vec![(IDS["publisher"].clone(), 2.into())]
                    .into_iter()
                    .collect(),
                &vec![(IDS["publisher"].clone(), 1.into())]
                    .into_iter()
                    .collect()
            ) >= HEALTH_THRESHOLD
        );
    }

    #[test]
    fn get_health_the_approved_balance_tree_has_less_it_is_unhealthy() {
        let channel = get_dummy_channel(80);

        assert!(
            get_health(
                &channel,
                &vec![(IDS["publisher"].clone(), 80.into())]
                    .into_iter()
                    .collect(),
                &vec![(IDS["publisher"].clone(), 70.into())]
                    .into_iter()
                    .collect()
            ) < HEALTH_THRESHOLD
        );
    }

    #[test]
    fn get_health_they_have_the_same_sum_but_different_entities_are_earning() {
        let channel = get_dummy_channel(80);

        assert!(
            get_health(
                &channel,
                &vec![(IDS["publisher"].clone(), 80.into())]
                    .into_iter()
                    .collect(),
                &vec![(IDS["publisher2"].clone(), 80.into())]
                    .into_iter()
                    .collect()
            ) < HEALTH_THRESHOLD
        );

        assert!(
            get_health(
                &channel,
                &vec![(IDS["publisher"].clone(), 80.into())]
                    .into_iter()
                    .collect(),
                &vec![
                    (IDS["publisher2"].clone(), 40.into()),
                    (IDS["publisher"].clone(), 40.into())
                ]
                .into_iter()
                .collect()
            ) < HEALTH_THRESHOLD
        );

        assert!(
            get_health(
                &channel,
                &vec![(IDS["publisher"].clone(), 80.into())]
                    .into_iter()
                    .collect(),
                &vec![
                    (IDS["publisher2"].clone(), 20.into()),
                    (IDS["publisher"].clone(), 60.into())
                ]
                .into_iter()
                .collect()
            ) < HEALTH_THRESHOLD
        );

        assert!(
            get_health(
                &channel,
                &vec![(IDS["publisher"].clone(), 80.into())]
                    .into_iter()
                    .collect(),
                &vec![
                    (IDS["publisher2"].clone(), 2.into()),
                    (IDS["publisher"].clone(), 78.into())
                ]
                .into_iter()
                .collect()
            ) >= HEALTH_THRESHOLD
        );

        assert!(
            get_health(
                &channel,
                &vec![
                    (IDS["publisher"].clone(), 100.into()),
                    (IDS["publisher2"].clone(), 1.into())
                ]
                .into_iter()
                .collect(),
                &vec![(IDS["publisher"].clone(), 100.into())]
                    .into_iter()
                    .collect()
            ) >= HEALTH_THRESHOLD
        );
    }
}
