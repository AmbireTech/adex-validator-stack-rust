use primitives::{UnifiedMap, UnifiedNum};

pub fn is_valid_transition(
    all_spenders_sum: UnifiedNum,
    prev: &UnifiedMap,
    next: &UnifiedMap,
) -> Option<bool> {
    let sum_prev = prev.values().sum::<Option<_>>()?;
    let sum_next = next.values().sum::<Option<_>>()?;

    let prev_checks = prev.iter().all(|(acc, bal)| match next.get(acc) {
        Some(next_bal) => next_bal >= bal,
        None => false,
    });

    // no need to check if there are negative balances as we don't allow them using UnifiedNum
    Some(sum_next >= sum_prev && sum_next <= all_spenders_sum && prev_checks)
}

pub fn get_health(
    all_spenders_sum: UnifiedNum,
    our: &UnifiedMap,
    approved: &UnifiedMap,
) -> Option<u64> {
    let sum_our: UnifiedNum = our.values().sum::<Option<_>>()?;

    let zero = UnifiedNum::from(0);
    let sum_approved_mins = our
        .iter()
        .map(|(acc, val)| val.min(approved.get(acc).unwrap_or(&zero)))
        .sum::<Option<_>>()?;

    if sum_approved_mins >= sum_our {
        return Some(1_000);
    }

    let diff = sum_our - sum_approved_mins;
    let health_penalty = diff * UnifiedNum::from(1_000) / all_spenders_sum;

    Some(1_000 - health_penalty.to_u64())
}

#[cfg(test)]
mod test {
    use primitives::test_util::{PUBLISHER, PUBLISHER_2};

    use super::*;

    const HEALTH_THRESHOLD: u64 = 950;

    #[test]
    fn is_valid_transition_empty_to_empty() {
        assert!(
            is_valid_transition(
                UnifiedNum::from_u64(100),
                &UnifiedMap::default(),
                &UnifiedMap::default()
            )
            .expect("No overflow"),
            "is valid transition"
        )
    }

    #[test]
    fn is_valid_transition_a_valid_transition() {
        let next = vec![(*PUBLISHER, 100.into())].into_iter().collect();

        assert!(
            is_valid_transition(UnifiedNum::from_u64(100), &UnifiedMap::default(), &next)
                .expect("No overflow"),
            "is valid transition"
        )
    }

    #[test]
    fn is_valid_transition_more_funds_than_all_spenders_sum() {
        let next = vec![(*PUBLISHER, 51.into()), (*PUBLISHER_2, 50.into())]
            .into_iter()
            .collect();

        assert!(
            !is_valid_transition(UnifiedNum::from_u64(100), &UnifiedMap::default(), &next)
                .expect("No overflow"),
            "not a valid transition"
        );
    }

    #[test]
    fn is_valid_transition_single_value_is_lower() {
        let prev = vec![(*PUBLISHER, 55.into())].into_iter().collect();

        let next = vec![(*PUBLISHER, 54.into())].into_iter().collect();

        assert!(
            !is_valid_transition(UnifiedNum::from_u64(100), &prev, &next).expect("No overflow"),
            "not a valid transition"
        );
    }

    #[test]
    fn is_valid_transition_a_value_is_lower_but_overall_sum_is_higher() {
        let prev = vec![(*PUBLISHER, 55.into())].into_iter().collect();

        let next = vec![(*PUBLISHER, 54.into()), (*PUBLISHER_2, 3.into())]
            .into_iter()
            .collect();

        assert!(
            !is_valid_transition(UnifiedNum::from_u64(100), &prev, &next).expect("No overflow"),
            "not a valid transition"
        );
    }

    #[test]
    fn is_valid_transition_overall_sum_is_lower() {
        let prev = vec![(*PUBLISHER, 54.into()), (*PUBLISHER_2, 3.into())]
            .into_iter()
            .collect();

        let next = vec![(*PUBLISHER, 54.into())].into_iter().collect();

        assert!(
            !is_valid_transition(UnifiedNum::from_u64(100), &prev, &next).expect("No overflow"),
            "not a valid transition"
        );
    }

    #[test]
    fn is_valid_transition_overall_sum_is_the_same_but_we_remove_an_entry() {
        let prev = vec![(*PUBLISHER, 54.into()), (*PUBLISHER_2, 3.into())]
            .into_iter()
            .collect();

        let next = vec![(*PUBLISHER, 57.into())].into_iter().collect();

        assert!(
            !is_valid_transition(UnifiedNum::from_u64(100), &prev, &next).expect("No overflow"),
            "not a valid transition"
        );
    }

    #[test]
    fn get_health_the_approved_balance_tree_gte_our_accounting_is_healthy() {
        let all_spenders_sum = UnifiedNum::from(50);
        let our = vec![(*PUBLISHER, 50.into())].into_iter().collect();
        assert!(
            get_health(all_spenders_sum, &our, &our).expect("Should not overflow")
                >= HEALTH_THRESHOLD
        );

        assert!(
            get_health(
                all_spenders_sum,
                &our,
                &vec![(*PUBLISHER, 60.into())].into_iter().collect()
            )
            .expect("Should not overflow")
                >= HEALTH_THRESHOLD
        );
    }

    #[test]
    fn get_health_the_approved_balance_tree_is_positive_our_accounting_is_0_and_it_is_healthy() {
        let approved = vec![(*PUBLISHER, 50.into())].into_iter().collect();

        assert!(
            get_health(UnifiedNum::from(50), &UnifiedMap::default(), &approved)
                .expect("Should not overflow")
                >= HEALTH_THRESHOLD
        );
    }

    #[test]
    fn get_health_the_approved_balance_tree_has_less_but_within_margin_it_is_healthy() {
        let all_spenders_sum = UnifiedNum::from(80);

        assert!(
            get_health(
                all_spenders_sum,
                &vec![(*PUBLISHER, 80.into())].into_iter().collect(),
                &vec![(*PUBLISHER, 79.into())].into_iter().collect()
            )
            .expect("Should not overflow")
                >= HEALTH_THRESHOLD
        );

        assert!(
            get_health(
                all_spenders_sum,
                &vec![(*PUBLISHER, 2.into())].into_iter().collect(),
                &vec![(*PUBLISHER, 1.into())].into_iter().collect()
            )
            .expect("Should not overflow")
                >= HEALTH_THRESHOLD
        );
    }

    #[test]
    fn get_health_the_approved_balance_tree_has_less_it_is_unhealthy() {
        assert!(
            get_health(
                UnifiedNum::from(80),
                &vec![(*PUBLISHER, 80.into())].into_iter().collect(),
                &vec![(*PUBLISHER, 70.into())].into_iter().collect()
            )
            .expect("Should not overflow")
                < HEALTH_THRESHOLD
        );
    }

    #[test]
    fn get_health_they_have_the_same_sum_but_different_entities_are_earning() {
        let all_spenders_sum = UnifiedNum::from(80);

        assert!(
            get_health(
                all_spenders_sum,
                &vec![(*PUBLISHER, 80.into())].into_iter().collect(),
                &vec![(*PUBLISHER_2, 80.into())].into_iter().collect()
            )
            .expect("Should not overflow")
                < HEALTH_THRESHOLD
        );

        assert!(
            get_health(
                all_spenders_sum,
                &vec![(*PUBLISHER, 80.into())].into_iter().collect(),
                &vec![(*PUBLISHER_2, 40.into()), (*PUBLISHER, 40.into())]
                    .into_iter()
                    .collect()
            )
            .expect("Should not overflow")
                < HEALTH_THRESHOLD
        );

        assert!(
            get_health(
                all_spenders_sum,
                &vec![(*PUBLISHER, 80.into())].into_iter().collect(),
                &vec![(*PUBLISHER_2, 20.into()), (*PUBLISHER, 60.into())]
                    .into_iter()
                    .collect()
            )
            .expect("Should not overflow")
                < HEALTH_THRESHOLD
        );

        assert!(
            get_health(
                all_spenders_sum,
                &vec![(*PUBLISHER, 80.into())].into_iter().collect(),
                &vec![(*PUBLISHER_2, 2.into()), (*PUBLISHER, 78.into())]
                    .into_iter()
                    .collect()
            )
            .expect("Should not overflow")
                >= HEALTH_THRESHOLD
        );

        assert!(
            get_health(
                all_spenders_sum,
                &vec![(*PUBLISHER, 100.into()), (*PUBLISHER_2, 1.into())]
                    .into_iter()
                    .collect(),
                &vec![(*PUBLISHER, 100.into())].into_iter().collect()
            )
            .expect("Should not overflow")
                >= HEALTH_THRESHOLD
        );
    }
}
