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

pub fn is_healthy(
    channel: &Channel,
    our: &BalancesMap,
    approved: &BalancesMap,
    health_threshold: BigNum,
) -> bool {
    let sum_our: BigNum = our.values().sum();

    let zero = BigNum::from(0);
    let sum_approved_mins = our
        .iter()
        .map(|(acc, val)| val.min(approved.get(acc).unwrap_or(&zero)))
        .sum();

    if sum_approved_mins >= sum_our {
        return true;
    }

    let deposit = &channel.deposit_amount;
    let health_threshold_neg = BigNum::from(1_000) - health_threshold;
    let acceptable_difference = deposit * &health_threshold_neg / &BigNum::from(1_000);

    sum_our - sum_approved_mins < acceptable_difference
}

#[cfg(test)]
mod test {
    use super::*;
    use primitives::channel::fixtures::get_channel;

    fn health_threshold() -> BigNum {
        950.into()
    }

    fn get_dummy_channel<T: Into<BigNum>>(deposit: T) -> Channel {
        let mut channel = get_channel("channel", &None, None);
        channel.deposit_amount = deposit.into();

        channel
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
        let next = vec![("a".into(), 100.into())].into_iter().collect();

        assert!(
            is_valid_transition(&get_dummy_channel(100), &BalancesMap::default(), &next,),
            "is valid transition"
        )
    }

    #[test]
    fn is_valid_transition_more_funds_than_dummy_channel() {
        let next = vec![("a".into(), 51.into()), ("b".into(), 50.into())]
            .into_iter()
            .collect();

        assert!(
            !is_valid_transition(&get_dummy_channel(100), &BalancesMap::default(), &next),
            "not a valid transition"
        );
    }

    #[test]
    fn is_valid_transition_single_value_is_lower() {
        let prev = vec![("a".into(), 55.into())].into_iter().collect();

        let next = vec![("a".into(), 54.into())].into_iter().collect();

        assert!(
            !is_valid_transition(&get_dummy_channel(100), &prev, &next),
            "not a valid transition"
        );
    }

    #[test]
    fn is_valid_transition_a_value_is_lower_but_overall_sum_is_higher() {
        let prev = vec![("a".into(), 55.into())].into_iter().collect();

        let next = vec![("a".into(), 54.into()), ("b".into(), 3.into())]
            .into_iter()
            .collect();

        assert!(
            !is_valid_transition(&get_dummy_channel(100), &prev, &next),
            "not a valid transition"
        );
    }

    #[test]
    fn is_valid_transition_overall_sum_is_lower() {
        let prev = vec![("a".into(), 54.into()), ("b".into(), 3.into())]
            .into_iter()
            .collect();

        let next = vec![("a".into(), 54.into())].into_iter().collect();

        assert!(
            !is_valid_transition(&get_dummy_channel(100), &prev, &next),
            "not a valid transition"
        );
    }

    #[test]
    fn is_valid_transition_overall_sum_is_the_same_but_we_remove_an_entry() {
        let prev = vec![("a".into(), 54.into()), ("b".into(), 3.into())]
            .into_iter()
            .collect();

        let next = vec![("a".into(), 57.into())].into_iter().collect();

        assert!(
            !is_valid_transition(&get_dummy_channel(100), &prev, &next),
            "not a valid transition"
        );
    }

    #[test]
    fn is_valid_transition_transition_to_a_state_with_a_negative_number() {
        let prev = vec![("a".into(), 54.into()), ("b".into(), 3.into())]
            .into_iter()
            .collect();

        let next = vec![("a".into(), 57.into())].into_iter().collect();

        assert!(
            !is_valid_transition(&get_dummy_channel(100), &prev, &next),
            "not a valid transition"
        );
    }

    #[test]
    fn is_healthy_the_approved_balance_tree_gte_our_accounting_is_healthy() {
        let channel = get_dummy_channel(50);
        let our = vec![("a".into(), 50.into())].into_iter().collect();
        assert!(is_healthy(&channel, &our, &our, health_threshold()));

        assert!(is_healthy(
            &channel,
            &our,
            &vec![("a".into(), 60.into())].into_iter().collect(),
            health_threshold()
        ));
    }

    #[test]
    fn is_healthy_the_approved_balance_tree_is_positive_our_accounting_is_0_and_it_is_healthy() {
        let approved = vec![("a".into(), 50.into())].into_iter().collect();

        assert!(is_healthy(
            &get_dummy_channel(50),
            &BalancesMap::default(),
            &approved,
            health_threshold()
        ));
    }

    #[test]
    fn is_healthy_the_approved_balance_tree_has_less_but_within_margin_it_is_healthy() {
        let channel = get_dummy_channel(80);

        assert!(is_healthy(
            &channel,
            &vec![("a".into(), 80.into())].into_iter().collect(),
            &vec![("a".into(), 79.into())].into_iter().collect(),
            health_threshold()
        ));

        assert!(is_healthy(
            &channel,
            &vec![("a".into(), 2.into())].into_iter().collect(),
            &vec![("a".into(), 1.into())].into_iter().collect(),
            health_threshold()
        ));
    }

    #[test]
    fn is_healthy_the_approved_balance_tree_has_less_it_is_unhealthy() {
        let channel = get_dummy_channel(80);

        assert!(!is_healthy(
            &channel,
            &vec![("a".into(), 80.into())].into_iter().collect(),
            &vec![("a".into(), 70.into())].into_iter().collect(),
            health_threshold()
        ));
    }

    #[test]
    fn is_healthy_they_have_the_same_sum_but_different_entities_are_earning() {
        let channel = get_dummy_channel(80);

        assert!(!is_healthy(
            &channel,
            &vec![("a".into(), 80.into())].into_iter().collect(),
            &vec![("b".into(), 80.into())].into_iter().collect(),
            health_threshold()
        ));

        assert!(!is_healthy(
            &channel,
            &vec![("a".into(), 80.into())].into_iter().collect(),
            &vec![("b".into(), 40.into()), ("a".into(), 40.into())]
                .into_iter()
                .collect(),
            health_threshold()
        ));

        assert!(!is_healthy(
            &channel,
            &vec![("a".into(), 80.into())].into_iter().collect(),
            &vec![("b".into(), 20.into()), ("a".into(), 60.into())]
                .into_iter()
                .collect(),
            health_threshold()
        ));

        assert!(is_healthy(
            &channel,
            &vec![("a".into(), 80.into())].into_iter().collect(),
            &vec![("b".into(), 2.into()), ("a".into(), 78.into())]
                .into_iter()
                .collect(),
            health_threshold()
        ));

        assert!(is_healthy(
            &channel,
            &vec![("a".into(), 100.into()), ("b".into(), 1.into())]
                .into_iter()
                .collect(),
            &vec![("a".into(), 100.into())].into_iter().collect(),
            health_threshold()
        ));
    }
}
