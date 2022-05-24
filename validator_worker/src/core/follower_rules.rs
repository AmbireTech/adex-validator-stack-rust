use primitives::{UnifiedMap, UnifiedNum};

static MAX_HEALTH: u64 = 1_000;

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

    let sum_approved_mins = our
        .iter()
        .map(|(acc, val)| val.min(approved.get(acc).unwrap_or(&UnifiedNum::ZERO)))
        .sum::<Option<_>>()?;

    if sum_approved_mins >= sum_our {
        return Some(MAX_HEALTH);
    }
    let diff = sum_our - sum_approved_mins;

    // it's easier to work with `u64` instead of later dividing the `UnifiedNum`'s inner `u64` with `10.pow(UnifiedNum::PRECISION)`
    let health_penalty = diff
        .to_u64()
        .checked_mul(MAX_HEALTH)?
        .checked_div(all_spenders_sum.to_u64())?;

    Some(MAX_HEALTH - health_penalty)
}

#[cfg(test)]
mod test {
    use primitives::{
        test_util::{PUBLISHER, PUBLISHER_2},
        unified_num::FromWhole,
    };

    use super::*;

    const HEALTH_THRESHOLD: u64 = 950;

    #[test]
    fn is_valid_transition_empty_to_empty() {
        assert!(
            is_valid_transition(
                UnifiedNum::from_whole(100_u64),
                &UnifiedMap::default(),
                &UnifiedMap::default()
            )
            .expect("Should return health and not overflow"),
            "is valid transition"
        )
    }

    #[test]
    fn is_valid_transition_a_valid_transition() {
        let next = vec![(*PUBLISHER, UnifiedNum::from_whole(100_u64))]
            .into_iter()
            .collect();

        assert!(
            is_valid_transition(
                UnifiedNum::from_whole(100_u64),
                &UnifiedMap::default(),
                &next
            )
            .expect("Should return health and not overflow"),
            "is valid transition"
        )
    }

    #[test]
    fn is_valid_transition_more_funds_than_all_spenders_sum() {
        let next = vec![
            (*PUBLISHER, UnifiedNum::from_whole(51_u64)),
            (*PUBLISHER_2, UnifiedNum::from_whole(50_u64)),
        ]
        .into_iter()
        .collect();

        assert!(
            !is_valid_transition(
                UnifiedNum::from_whole(100_u64),
                &UnifiedMap::default(),
                &next
            )
            .expect("Should return health and not overflow"),
            "not a valid transition"
        );
    }

    #[test]
    fn is_valid_transition_single_value_is_lower() {
        let prev = vec![(*PUBLISHER, UnifiedNum::from_whole(55_u64))]
            .into_iter()
            .collect();

        let next = vec![(*PUBLISHER, UnifiedNum::from_whole(54_u64))]
            .into_iter()
            .collect();

        assert!(
            !is_valid_transition(UnifiedNum::from_whole(100_u64), &prev, &next)
                .expect("Should return health and not overflow"),
            "not a valid transition"
        );
    }

    #[test]
    fn is_valid_transition_a_value_is_lower_but_overall_sum_is_higher() {
        let prev = vec![(*PUBLISHER, UnifiedNum::from_whole(55_u64))]
            .into_iter()
            .collect();

        let next = vec![
            (*PUBLISHER, UnifiedNum::from_whole(54_u64)),
            (*PUBLISHER_2, UnifiedNum::from_whole(3_u64)),
        ]
        .into_iter()
        .collect();

        assert!(
            !is_valid_transition(UnifiedNum::from_whole(100_u64), &prev, &next)
                .expect("Should return health and not overflow"),
            "not a valid transition"
        );
    }

    #[test]
    fn is_valid_transition_overall_sum_is_lower() {
        let prev = vec![
            (*PUBLISHER, UnifiedNum::from_whole(54_u64)),
            (*PUBLISHER_2, UnifiedNum::from_whole(3_u64)),
        ]
        .into_iter()
        .collect();

        let next = vec![(*PUBLISHER, UnifiedNum::from_whole(54_u64))]
            .into_iter()
            .collect();

        assert!(
            !is_valid_transition(UnifiedNum::from_whole(100_u64), &prev, &next)
                .expect("Should return health and not overflow"),
            "not a valid transition"
        );
    }

    #[test]
    fn is_valid_transition_overall_sum_is_the_same_but_we_remove_an_entry() {
        let prev = vec![
            (*PUBLISHER, UnifiedNum::from_whole(54_u64)),
            (*PUBLISHER_2, UnifiedNum::from_whole(3_u64)),
        ]
        .into_iter()
        .collect();

        let next = vec![(*PUBLISHER, UnifiedNum::from_whole(57_u64))]
            .into_iter()
            .collect();

        assert!(
            !is_valid_transition(UnifiedNum::from_whole(100_u64), &prev, &next)
                .expect("Should return health and not overflow"),
            "not a valid transition"
        );
    }

    #[test]
    fn get_health_the_approved_balance_tree_gte_our_accounting_is_healthy() {
        let all_spenders_sum = UnifiedNum::from_whole(50_u64);
        let our = vec![(*PUBLISHER, UnifiedNum::from_whole(50_u64))]
            .into_iter()
            .collect();

        {
            let health = get_health(all_spenders_sum, &our, &our)
                .expect("Should return health and not overflow");
            assert!(health >= HEALTH_THRESHOLD);
        }
        {
            let health = get_health(
                all_spenders_sum,
                &our,
                &vec![(*PUBLISHER, UnifiedNum::from_whole(60_u64))]
                    .into_iter()
                    .collect(),
            )
            .expect("Should return health and not overflow");
            assert!(health >= HEALTH_THRESHOLD);
        }
    }

    #[test]
    fn get_health_the_approved_balance_tree_is_positive_our_accounting_is_0_and_it_is_healthy() {
        let approved = vec![(*PUBLISHER, UnifiedNum::from_whole(50_u64))]
            .into_iter()
            .collect();

        let health = get_health(
            UnifiedNum::from_whole(50_u64),
            &UnifiedMap::default(),
            &approved,
        )
        .expect("Should return health and not overflow");

        assert_eq!(1000, health);

        assert!(health >= HEALTH_THRESHOLD, "healthy");
    }

    #[test]
    fn get_health_the_approved_balance_tree_has_less_but_within_margin_it_is_healthy() {
        let all_spenders_sum = UnifiedNum::from_whole(80_u64);

        {
            let health = get_health(
                all_spenders_sum,
                &vec![(*PUBLISHER, UnifiedNum::from_whole(80_u64))]
                    .into_iter()
                    .collect(),
                &vec![(*PUBLISHER, UnifiedNum::from_whole(79_u64))]
                    .into_iter()
                    .collect(),
            )
            .expect("Should return health and not overflow");

            assert_eq!(health, 988, "Very small difference from all spender sum");
            assert!(health >= HEALTH_THRESHOLD, "healthy");
        }

        {
            let health = get_health(
                all_spenders_sum,
                &vec![(*PUBLISHER, UnifiedNum::from_whole(2_u64))]
                    .into_iter()
                    .collect(),
                &vec![(*PUBLISHER, UnifiedNum::from_whole(1_u64))]
                    .into_iter()
                    .collect(),
            )
            .expect("Should return health and not overflow");
            assert_eq!(health, 988, "Major difference from all spenders sum");

            assert!(health >= HEALTH_THRESHOLD, "healthy");
        }
    }

    #[test]
    fn get_health_the_approved_balance_tree_has_less_it_is_unhealthy() {
        let health = get_health(
            UnifiedNum::from_whole(80_u64),
            &vec![(*PUBLISHER, UnifiedNum::from_whole(80_u64))]
                .into_iter()
                .collect(),
            &vec![(*PUBLISHER, UnifiedNum::from_whole(70_u64))]
                .into_iter()
                .collect(),
        )
        .expect("Should return health and not overflow");

        assert_eq!(875, health);
        assert!(health < HEALTH_THRESHOLD, "unhealthy");
    }

    #[test]
    fn get_health_they_have_the_same_sum_but_different_entities_are_earning() {
        let all_spenders_sum = UnifiedNum::from_whole(80_u64);

        // Unhealthy
        {
            let health = get_health(
                all_spenders_sum,
                &vec![(*PUBLISHER, UnifiedNum::from_whole(80_u64))]
                    .into_iter()
                    .collect(),
                &vec![(*PUBLISHER_2, UnifiedNum::from_whole(80_u64))]
                    .into_iter()
                    .collect(),
            )
            .expect("Should return health and not overflow");
            assert_eq!(health, 0, "None of the spenders match in ours/approved");
            assert!(health < HEALTH_THRESHOLD, "unhealthy");
        }

        // Unhealthy
        {
            let health = get_health(
                all_spenders_sum,
                &vec![(*PUBLISHER, UnifiedNum::from_whole(80_u64))]
                    .into_iter()
                    .collect(),
                &vec![
                    (*PUBLISHER_2, UnifiedNum::from_whole(40_u64)),
                    (*PUBLISHER, UnifiedNum::from_whole(40_u64)),
                ]
                .into_iter()
                .collect(),
            )
            .expect("Should return health and not overflow");
            assert_eq!(health, 500, "Exactly half of the health");
            assert!(health < HEALTH_THRESHOLD, "unhealthy");
        }

        // Unhealthy
        {
            let health = get_health(
                all_spenders_sum,
                &vec![(*PUBLISHER, UnifiedNum::from_whole(80_u64))]
                    .into_iter()
                    .collect(),
                &vec![
                    (*PUBLISHER_2, UnifiedNum::from_whole(20_u64)),
                    (*PUBLISHER, UnifiedNum::from_whole(60_u64)),
                ]
                .into_iter()
                .collect(),
            )
            .expect("Should return health and not overflow");
            assert_eq!(health, 750, "One fourth expected");
            assert!(health < HEALTH_THRESHOLD, "unhealthy");
        }

        // Healthy
        {
            let health = get_health(
                all_spenders_sum,
                &vec![(*PUBLISHER, UnifiedNum::from_whole(80_u64))]
                    .into_iter()
                    .collect(),
                &vec![
                    (*PUBLISHER_2, UnifiedNum::from_whole(2_u64)),
                    (*PUBLISHER, UnifiedNum::from_whole(78_u64)),
                ]
                .into_iter()
                .collect(),
            )
            .expect("Should return health and not overflow");

            assert_eq!(health, 975,);
            assert!(health >= HEALTH_THRESHOLD, "healthy");
        }

        // Healthy
        {
            let health = get_health(
                all_spenders_sum,
                &vec![
                    (*PUBLISHER, UnifiedNum::from_whole(100_u64)),
                    (*PUBLISHER_2, UnifiedNum::from_whole(1_u64)),
                ]
                .into_iter()
                .collect(),
                &vec![(*PUBLISHER, UnifiedNum::from_whole(100_u64))]
                    .into_iter()
                    .collect(),
            )
            .expect("Should return health and not overflow");
            assert_eq!(health, 988);
            assert!(health >= HEALTH_THRESHOLD, "healthy");
        }
    }
}
