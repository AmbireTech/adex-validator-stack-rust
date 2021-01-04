use chrono::{TimeZone, Utc};

use super::*;
use crate::{
    targeting::input,
    util::tests::prep_db::{DUMMY_CHANNEL, DUMMY_IPFS, IDS},
    BalancesMap,
};

fn get_default_input() -> Input {
    let input_balances = BalancesMap::default();

    let init_input = Input {
        ad_view: Some(input::AdView {
            seconds_since_campaign_impression: 10,
            has_custom_preferences: false,
            navigator_language: "bg".to_string(),
        }),
        global: input::Global {
            ad_slot_id: "ad_slot_id Value".to_string(),
            ad_slot_type: "ad_slot_type Value".to_string(),
            publisher_id: IDS["leader"],
            country: Some("bg".to_string()),
            event_type: "IMPRESSION".to_string(),
            seconds_since_epoch: Utc.ymd(2020, 11, 06).and_hms(12, 0, 0),
            user_agent_os: Some("os".to_string()),
            user_agent_browser_family: Some("family".to_string()),
        },
        channel: None,
        balances: None,
        ad_unit_id: Some(DUMMY_IPFS[0].clone()),
        ad_slot: None,
    };

    // Set the Channel, Balances and AdUnit for the Input
    init_input
        .with_channel(DUMMY_CHANNEL.clone())
        .with_balances(input_balances)
}

mod rules_test {
    use super::{Function, Rule, Rules, Value};
    use serde_json::{from_value, json};

    #[test]
    fn test_rules_should_be_empty_when_single_invalid_rule() {
        let rule = json!([
            {
                "onlyShowIf": {
                    "undefined": [
                        [],
                        {"get":"userAgentOS"}
                    ]
                }
            }
        ]);

        let deser = from_value::<Rules>(rule).expect("should deserialize by skipping invalid rule");

        assert!(deser.0.is_empty())
    }

    #[test]
    fn test_rules_should_not_be_empty_when_one_invalid_rule() {
        let rule = json!([
            {
                "intersects": [
                    {"get": "adSlot.categories"},
                    ["News", "Bitcoin"]
                ]
            },
            {
                "onlyShowIf": {
                    "undefined": [
                        [],
                        {"get":"userAgentOS"}
                    ]
                }
            }
        ]);

        let deser = from_value::<Rules>(rule).expect("should deserialize by skipping invalid rule");

        assert_eq!(1, deser.0.len());

        let expected = Rule::Function(Function::new_intersects(
            Rule::Function(Function::new_get("adSlot.categories")),
            Rule::Value(Value::Array(vec![
                Value::new_string("News"),
                Value::new_string("Bitcoin"),
            ])),
        ));
        assert_eq!(expected, deser.0[0])
    }
}

mod dsl_test {
    use super::*;

    #[test]
    fn deserialize_intersects_with_get_rule() {
        let json = r#"{"intersects": [{ "get": "adSlot.categories" }, ["News", "Bitcoin"]]}"#;

        let parsed_rule = serde_json::from_str::<Rule>(json).expect("Should deserialize");

        let expected = Rule::Function(Function::new_intersects(
            Rule::Function(Function::new_get("adSlot.categories")),
            Rule::Value(Value::Array(vec![
                Value::new_string("News"),
                Value::new_string("Bitcoin"),
            ])),
        ));

        assert_eq!(expected, parsed_rule)
    }

    /// ```json
    /// {
    ///   "intersects": [
    ///     {
    ///       "get": "adSlot.categories"
    ///     },
    ///     [
    ///       "News",
    ///       "Bitcoin"
    ///     ]
    ///   ]
    /// }
    /// ```
    #[test]
    fn test_intersects_eval() {
        let mut input = get_default_input();
        input.ad_slot = Some(input::AdSlot {
            categories: vec!["Bitcoin".to_string(), "Ethereum".to_string()],
            hostname: Default::default(),
            alexa_rank: Some(0.0),
        });

        let mut output = Output {
            show: true,
            boost: 1.0,
            price: Default::default(),
        };

        let categories = vec![Value::new_string("News"), Value::new_string("Bitcoin")];

        let rules = Rule::Function(Function::new_intersects(
            Function::new_get("adSlot.categories"),
            Value::Array(categories),
        ));

        let result = rules.eval(&input, &mut output).expect("Should eval rules");

        assert_eq!(
            Value::Bool(true),
            result.expect("Should return Non-NULL result!")
        );

        let mut input = get_default_input();
        input.ad_slot = Some(input::AdSlot {
            categories: vec!["Advertisement".to_string(), "Programming".to_string()],
            hostname: Default::default(),
            alexa_rank: Some(0.0),
        });

        let result = rules.eval(&input, &mut output).expect("Should eval rules");

        assert_eq!(
            Value::Bool(false),
            result.expect("Should return Non-NULL result!")
        );
    }

    #[test]
    fn test_and_eval() {
        let input = get_default_input();
        let mut output = Output {
            show: true,
            boost: 1.0,
            price: Default::default(),
        };

        let cases = [
            (true, true, true),
            (false, false, false),
            (false, true, false),
            (true, false, false),
        ];

        for (lhs, rhs, expected) in cases.iter() {
            let rule = Rule::Function(Function::new_and(Value::Bool(*lhs), Value::Bool(*rhs)));
            let expected = Some(Value::Bool(*expected));

            assert_eq!(Ok(expected), rule.eval(&input, &mut output));
        }
    }

    #[test]
    fn test_if_eval() {
        let input = get_default_input();

        let mut output = Output {
            show: true,
            boost: 1.0,
            price: Default::default(),
        };

        let then = Value::String("yes".to_string());

        let rule = Rule::Function(Function::new_if(Value::Bool(true), then.clone()));

        assert_eq!(Ok(Some(then.clone())), rule.eval(&input, &mut output));

        let rule = Rule::Function(Function::new_if(Value::Bool(false), then));

        assert_eq!(Ok(None), rule.eval(&input, &mut output));
    }

    #[test]
    fn test_bn_eval_from_actual_number_value_string_bignum_or_number() {
        let input = get_default_input();
        let mut output = Output {
            show: true,
            boost: 1.0,
            price: Default::default(),
        };

        let cases = vec![
            (Value::new_string("1000"), Value::BigNum(1000.into())),
            (Value::new_number(2_000), Value::BigNum(2_000.into())),
            (Value::BigNum(3.into()), Value::BigNum(3.into())),
            // rounded floats should work!
            (
                Value::Number(Number::from_f64(40.0).expect("should create float number")),
                Value::BigNum(40.into()),
            ),
        ];

        for (from, expected) in cases.into_iter() {
            let rule = Rule::Function(Function::new_bn(from));

            assert_eq!(Ok(Some(expected)), rule.eval(&input, &mut output));
        }
    }

    #[test]
    fn test_bn_eval_from_actual_incorrect_value() {
        let input = get_default_input();
        let mut output = Output {
            show: true,
            boost: 1.0,
            price: Default::default(),
        };

        let error_cases = vec![
            Value::new_string("text"),
            // BigNums can only be positive
            Value::new_number(-100),
            Value::Bool(true),
            Value::Array(vec![Value::Bool(false)]),
            Value::Number(Number::from_f64(2.5).expect("should create float number")),
        ];

        for error_case in error_cases.into_iter() {
            let rule = Rule::Function(Function::new_bn(error_case));

            assert_eq!(Err(Error::TypeError), rule.eval(&input, &mut output));
        }
    }

    #[test]
    fn test_set_eval() {
        use crate::channel::{Pricing, PricingBounds};
        use crate::util::tests::prep_db::DUMMY_CHANNEL;

        let mut channel = DUMMY_CHANNEL.clone();
        channel.spec.pricing_bounds = Some(PricingBounds {
            impression: Some(Pricing {
                min: 1_000.into(),
                max: 2_000.into(),
            }),
            click: Some(Pricing {
                min: 3_000.into(),
                max: 4_000.into(),
            }),
        });

        let input = get_default_input();
        let mut output = Output::from(&channel);

        assert_eq!(Some(&BigNum::from(1_000)), output.price.get("IMPRESSION"));

        let set_to = Value::BigNum(BigNum::from(20));
        let rule = Rule::Function(Function::new_set("price.IMPRESSION", set_to));

        assert_eq!(Ok(None), rule.eval(&input, &mut output));

        assert_eq!(Some(&BigNum::from(20)), output.price.get("IMPRESSION"));
    }

    #[test]
    fn test_get_eval() {
        let input = get_default_input();
        let mut output = Output {
            show: true,
            boost: 42.0,
            price: Default::default(),
        };

        let input_country = Function::Get("country".to_string())
            .eval(&input, &mut output)
            .expect("Should get input.global.country");
        assert_eq!(Some(Value::String("bg".to_string())), input_country);

        let output_boost = Function::Get("boost".to_string())
            .eval(&input, &mut output)
            .expect("Should get output.boost");
        let expected_output_boost = Number::from_f64(42.0).expect("should create Number");

        assert_eq!(Some(Value::Number(expected_output_boost)), output_boost);
    }
}

mod math_functions {
    use super::*;

    #[test]
    fn test_div_eval() {
        let input = get_default_input();
        let mut output = Output {
            show: true,
            boost: 1.0,
            price: Default::default(),
        };

        let cases = vec![
            (
                Value::BigNum(100.into()),
                Value::BigNum(3.into()),
                Value::BigNum(33.into()),
            ),
            (
                Value::new_number(100),
                Value::BigNum(3.into()),
                Value::BigNum(33.into()),
            ),
            (
                Value::BigNum(100.into()),
                Value::new_number(3),
                Value::BigNum(33.into()),
            ),
            (
                Value::Number(Number::from_f64(100.0).expect("should create float number")),
                Value::Number(Number::from_f64(3.0).expect("should create float number")),
                Value::Number(
                    Number::from_f64(33.333_333_333_333_336).expect("should create float number"),
                ),
            ),
            (
                Value::new_number(10),
                Value::new_number(3),
                Value::new_number(10 / 3),
            ),
        ];

        for (lhs, rhs, expected) in cases.into_iter() {
            let rule = Rule::Function(Function::new_div(lhs, rhs));

            assert_eq!(Ok(Some(expected)), rule.eval(&input, &mut output));
        }
    }
    #[test]
    fn test_mul_eval() {
        let input = get_default_input();
        let mut output = Output {
            show: true,
            boost: 1.0,
            price: Default::default(),
        };

        let cases = vec![
            (
                Value::BigNum(3.into()),
                Value::BigNum(1000.into()),
                Value::BigNum(3000.into()),
            ),
            (
                Value::new_number(3),
                Value::BigNum(1000.into()),
                Value::BigNum(3000.into()),
            ),
            (
                Value::BigNum(3.into()),
                Value::new_number(1000),
                Value::BigNum(3000.into()),
            ),
            (
                Value::Number(Number::from_f64(0.5).expect("should create float number")),
                Value::Number(Number::from_f64(3000.0).expect("should create float number")),
                Value::Number(Number::from_f64(1500.0).expect("should create float number")),
            ),
            (
                Value::new_number(3),
                Value::new_number(1000),
                Value::new_number(3000),
            ),
        ];

        for (lhs, rhs, expected) in cases.into_iter() {
            let rule = Rule::Function(Function::new_mul(lhs, rhs));

            assert_eq!(Ok(Some(expected)), rule.eval(&input, &mut output));
        }
    }
    #[test]
    fn test_mod_eval() {
        let input = get_default_input();
        let mut output = Output {
            show: true,
            boost: 1.0,
            price: Default::default(),
        };

        let cases = vec![
            (
                Value::BigNum(10.into()),
                Value::BigNum(5.into()),
                Value::BigNum(0.into()),
            ),
            (
                Value::new_number(10),
                Value::BigNum(3.into()),
                Value::BigNum(1.into()),
            ),
            (
                Value::BigNum(10.into()),
                Value::new_number(4),
                Value::BigNum(2.into()),
            ),
            (
                Value::Number(Number::from_f64(10.0).expect("should create float number")),
                Value::Number(Number::from_f64(0.5).expect("should create float number")),
                Value::Number(Number::from_f64(0.0).expect("should create float number")),
            ),
            (
                Value::new_number(10),
                Value::new_number(1),
                Value::new_number(0),
            ),
        ];

        for (lhs, rhs, expected) in cases.into_iter() {
            let rule = Function::new_mod(lhs, rhs);

            assert_eq!(Ok(Some(expected)), rule.eval(&input, &mut output));
        }
    }
    #[test]
    fn test_add_eval() {
        let input = get_default_input();
        let mut output = Output {
            show: true,
            boost: 1.0,
            price: Default::default(),
        };

        let cases = vec![
            (
                Value::BigNum(2.into()),
                Value::BigNum(2.into()),
                Value::BigNum(4.into()),
            ),
            (
                Value::new_number(2),
                Value::BigNum(2.into()),
                Value::BigNum(4.into()),
            ),
            (
                Value::BigNum(2.into()),
                Value::new_number(2),
                Value::BigNum(4.into()),
            ),
            (
                Value::Number(Number::from_f64(2.2).expect("should create float number")),
                Value::Number(Number::from_f64(2.2).expect("should create float number")),
                Value::Number(Number::from_f64(4.4).expect("should create float number")),
            ),
            (
                Value::new_number(2),
                Value::new_number(2),
                Value::new_number(4),
            ),
        ];

        for (lhs, rhs, expected) in cases.into_iter() {
            let rule = Rule::Function(Function::new_add(lhs, rhs));

            assert_eq!(Ok(Some(expected)), rule.eval(&input, &mut output));
        }
    }
    #[test]
    fn test_sub_eval() {
        let input = get_default_input();
        let mut output = Output {
            show: true,
            boost: 1.0,
            price: Default::default(),
        };

        let cases = vec![
            (
                Value::BigNum(10.into()),
                Value::BigNum(2.into()),
                Value::BigNum(8.into()),
            ),
            (
                Value::new_number(10),
                Value::BigNum(10.into()),
                Value::BigNum(0.into()),
            ),
            (
                Value::BigNum(10.into()),
                Value::new_number(5),
                Value::BigNum(5.into()),
            ),
            (
                Value::Number(Number::from_f64(8.4).expect("should create float number")),
                Value::Number(Number::from_f64(2.7).expect("should create float number")),
                Value::Number(Number::from_f64(5.7).expect("should create float number")),
            ),
            (
                Value::new_number(10),
                Value::new_number(4),
                Value::new_number(6),
            ),
        ];

        for (lhs, rhs, expected) in cases.into_iter() {
            let rule = Rule::Function(Function::new_sub(lhs, rhs));

            assert_eq!(Ok(Some(expected)), rule.eval(&input, &mut output));
        }
    }
    #[test]
    fn test_min_eval() {
        let input = get_default_input();
        let mut output = Output {
            show: true,
            boost: 1.0,
            price: Default::default(),
        };

        let cases = vec![
            (
                Value::BigNum(100.into()),
                Value::BigNum(10.into()),
                Value::BigNum(10.into()),
            ),
            (
                Value::new_number(10),
                Value::BigNum(100.into()),
                Value::BigNum(10.into()),
            ),
            (
                Value::BigNum(10.into()),
                Value::new_number(10),
                Value::BigNum(10.into()),
            ),
            (
                Value::Number(Number::from_f64(0.1).expect("should create float number")),
                Value::Number(Number::from_f64(0.11).expect("should create float number")),
                Value::Number(Number::from_f64(0.1).expect("should create float number")),
            ),
            (
                Value::new_number(0),
                Value::new_number(0),
                Value::new_number(0),
            ),
        ];

        for (lhs, rhs, expected) in cases.into_iter() {
            let rule = Rule::Function(Function::new_min(lhs, rhs));

            assert_eq!(Ok(Some(expected)), rule.eval(&input, &mut output));
        }
    }
    #[test]
    fn test_max_eval() {
        let input = get_default_input();
        let mut output = Output {
            show: true,
            boost: 1.0,
            price: Default::default(),
        };

        let cases = vec![
            (
                Value::BigNum(100.into()),
                Value::BigNum(10.into()),
                Value::BigNum(100.into()),
            ),
            (
                Value::new_number(10),
                Value::BigNum(100.into()),
                Value::BigNum(100.into()),
            ),
            (
                Value::BigNum(10.into()),
                Value::new_number(10),
                Value::BigNum(10.into()),
            ),
            (
                Value::Number(Number::from_f64(0.1).expect("should create float number")),
                Value::Number(Number::from_f64(0.11).expect("should create float number")),
                Value::Number(Number::from_f64(0.11).expect("should create float number")),
            ),
            (
                Value::new_number(0),
                Value::new_number(0),
                Value::new_number(0),
            ),
        ];

        for (lhs, rhs, expected) in cases.into_iter() {
            let rule = Rule::Function(Function::new_max(lhs, rhs));

            assert_eq!(Ok(Some(expected)), rule.eval(&input, &mut output));
        }
    }
    #[test]
    fn test_lt_eval() {
        let input = get_default_input();
        let mut output = Output {
            show: true,
            boost: 1.0,
            price: Default::default(),
        };

        let cases = vec![
            (
                Value::BigNum(100.into()),
                Value::BigNum(10.into()),
                Value::Bool(false),
            ),
            (
                Value::new_number(100),
                Value::BigNum(100.into()),
                Value::Bool(false),
            ),
            (
                Value::BigNum(10.into()),
                Value::new_number(100),
                Value::Bool(true),
            ),
            (
                Value::Number(Number::from_f64(0.1).expect("should create float number")),
                Value::Number(Number::from_f64(0.11).expect("should create float number")),
                Value::Bool(true),
            ),
            (
                Value::new_number(0),
                Value::new_number(0),
                Value::Bool(false),
            ),
        ];

        for (lhs, rhs, expected) in cases.into_iter() {
            let rule = Rule::Function(Function::new_lt(lhs, rhs));

            assert_eq!(Ok(Some(expected)), rule.eval(&input, &mut output));
        }
    }
    #[test]
    fn test_lte_eval() {
        let input = get_default_input();
        let mut output = Output {
            show: true,
            boost: 1.0,
            price: Default::default(),
        };

        let cases = vec![
            (
                Value::BigNum(100.into()),
                Value::BigNum(10.into()),
                Value::Bool(false),
            ),
            (
                Value::new_number(100),
                Value::BigNum(100.into()),
                Value::Bool(true),
            ),
            (
                Value::BigNum(10.into()),
                Value::new_number(100),
                Value::Bool(true),
            ),
            (
                Value::Number(Number::from_f64(0.1).expect("should create float number")),
                Value::Number(Number::from_f64(0.11).expect("should create float number")),
                Value::Bool(true),
            ),
            (
                Value::new_number(0),
                Value::new_number(0),
                Value::Bool(true),
            ),
        ];

        for (lhs, rhs, expected) in cases.into_iter() {
            let rule = Rule::Function(Function::new_lte(lhs, rhs));

            assert_eq!(Ok(Some(expected)), rule.eval(&input, &mut output));
        }
    }
    #[test]
    fn test_gt_eval() {
        let input = get_default_input();
        let mut output = Output {
            show: true,
            boost: 1.0,
            price: Default::default(),
        };

        let cases = vec![
            (
                Value::BigNum(100.into()),
                Value::BigNum(10.into()),
                Value::Bool(true),
            ),
            (
                Value::new_number(100),
                Value::BigNum(100.into()),
                Value::Bool(false),
            ),
            (
                Value::BigNum(10.into()),
                Value::new_number(100),
                Value::Bool(false),
            ),
            (
                Value::Number(Number::from_f64(0.1).expect("should create float number")),
                Value::Number(Number::from_f64(0.11).expect("should create float number")),
                Value::Bool(false),
            ),
            (
                Value::new_number(0),
                Value::new_number(0),
                Value::Bool(false),
            ),
        ];

        for (lhs, rhs, expected) in cases.into_iter() {
            let rule = Rule::Function(Function::new_gt(lhs, rhs));

            assert_eq!(Ok(Some(expected)), rule.eval(&input, &mut output));
        }
    }
    #[test]
    fn test_gte_eval() {
        let input = get_default_input();
        let mut output = Output {
            show: true,
            boost: 1.0,
            price: Default::default(),
        };

        let cases = vec![
            (
                Value::BigNum(100.into()),
                Value::BigNum(10.into()),
                Value::Bool(true),
            ),
            (
                Value::new_number(100),
                Value::BigNum(100.into()),
                Value::Bool(true),
            ),
            (
                Value::BigNum(10.into()),
                Value::new_number(100),
                Value::Bool(false),
            ),
            (
                Value::Number(Number::from_f64(0.1).expect("should create float number")),
                Value::Number(Number::from_f64(0.11).expect("should create float number")),
                Value::Bool(false),
            ),
            (
                Value::new_number(0),
                Value::new_number(0),
                Value::Bool(true),
            ),
        ];

        for (lhs, rhs, expected) in cases.into_iter() {
            let rule = Rule::Function(Function::new_gte(lhs, rhs));

            assert_eq!(Ok(Some(expected)), rule.eval(&input, &mut output));
        }
    }
    #[test]
    fn test_between_eval() {
        let input = get_default_input();
        let mut output = Output {
            show: true,
            boost: 1.0,
            price: Default::default(),
        };

        let cases = vec![
            (
                Value::BigNum(10.into()),
                Value::BigNum(100.into()),
                Value::BigNum(1.into()),
                Value::Bool(false),
            ),
            (
                Value::BigNum(10.into()),
                Value::BigNum(100.into()),
                Value::BigNum(10.into()),
                Value::Bool(true),
            ),
            (
                Value::BigNum(10.into()),
                Value::BigNum(100.into()),
                Value::BigNum(50.into()),
                Value::Bool(true),
            ),
            (
                Value::BigNum(10.into()),
                Value::BigNum(100.into()),
                Value::BigNum(100.into()),
                Value::Bool(true),
            ),
            (
                Value::BigNum(10.into()),
                Value::BigNum(100.into()),
                Value::BigNum(1000.into()),
                Value::Bool(false),
            ),
        ];

        for (start, end, value, expected) in cases.into_iter() {
            let rule = Rule::Function(Function::new_between(start, end, value));

            assert_eq!(Ok(Some(expected)), rule.eval(&input, &mut output));
        }
    }
    #[test]
    fn test_muldiv_eval() {
        let input = get_default_input();
        let mut output = Output {
            show: true,
            boost: 1.0,
            price: Default::default(),
        };

        let rule = Rule::Function(Function::new_muldiv(
            Value::BigNum(10.into()),
            Value::BigNum(10.into()),
            Value::BigNum(2.into()),
        ));
        assert_eq!(
            Ok(Some(Value::BigNum(50.into()))),
            rule.eval(&input, &mut output)
        );
    }
}

mod control_flow_and_logic {
    use super::*;

    #[test]
    fn test_if_not_eval() {
        let input = get_default_input();
        let mut output = Output {
            show: true,
            boost: 1.0,
            price: Default::default(),
        };

        let then = Value::String("no".to_string());

        let rule = Rule::Function(Function::new_if_not(Value::Bool(false), then.clone()));

        assert_eq!(Ok(Some(then.clone())), rule.eval(&input, &mut output));

        let rule = Rule::Function(Function::new_if_not(Value::Bool(true), then));

        assert_eq!(Ok(None), rule.eval(&input, &mut output));
    }
    #[test]
    fn test_if_else() {
        let input = get_default_input();
        let mut output = Output {
            show: true,
            boost: 1.0,
            price: Default::default(),
        };

        let if_true = Value::String("is true".to_string());
        let if_false = Value::String("is false".to_string());

        let rule = Rule::Function(Function::new_if_else(
            Value::Bool(true),
            if_true.clone(),
            if_false.clone(),
        ));

        assert_eq!(Ok(Some(if_true.clone())), rule.eval(&input, &mut output));

        let rule = Rule::Function(Function::new_if_else(
            Value::Bool(false),
            if_true.clone(),
            if_false.clone(),
        ));

        assert_eq!(Ok(Some(if_false.clone())), rule.eval(&input, &mut output));
    }
    #[test]
    fn test_or_eval() {
        let input = get_default_input();
        let mut output = Output {
            show: true,
            boost: 1.0,
            price: Default::default(),
        };

        let cases = [
            (true, true, true),
            (false, false, false),
            (false, true, true),
            (true, false, true),
        ];

        for (lhs, rhs, expected) in cases.iter() {
            let rule = Rule::Function(Function::new_or(Value::Bool(*lhs), Value::Bool(*rhs)));
            let expected = Some(Value::Bool(*expected));

            assert_eq!(Ok(expected), rule.eval(&input, &mut output));
        }
    }
    #[test]
    fn test_xor_eval() {
        let input = get_default_input();
        let mut output = Output {
            show: true,
            boost: 1.0,
            price: Default::default(),
        };

        let cases = [
            (true, true, false),
            (false, false, false),
            (false, true, true),
            (true, false, true),
        ];

        for (lhs, rhs, expected) in cases.iter() {
            let rule = Rule::Function(Function::new_xor(Value::Bool(*lhs), Value::Bool(*rhs)));
            let expected = Some(Value::Bool(*expected));

            assert_eq!(Ok(expected), rule.eval(&input, &mut output));
        }
    }
    #[test]
    fn test_not_eval() {
        let input = get_default_input();
        let mut output = Output {
            show: true,
            boost: 1.0,
            price: Default::default(),
        };

        let cases = vec![(true, false), (false, true)];

        for (value, expected) in cases.into_iter() {
            let rule = Rule::Function(Function::new_not(Value::Bool(value)));
            let expected = Some(Value::Bool(expected));

            assert_eq!(Ok(expected), rule.eval(&input, &mut output));
        }
    }
    #[test]
    fn test_eq_eval() {
        let input = get_default_input();
        let mut output = Output {
            show: true,
            boost: 1.0,
            price: Default::default(),
        };

        let cases = [
            (Value::BigNum(1.into()), Value::BigNum(1.into()), true),
            (Value::BigNum(1.into()), Value::BigNum(2.into()), false),
            (
                Value::Number(Number::from_f64(3.33).expect("should create float")),
                Value::Number(Number::from_f64(3.33).expect("should create float")),
                true,
            ),
            (
                Value::Number(Number::from_f64(3.33).expect("should create float")),
                Value::Number(Number::from_f64(3.3).expect("should create float")),
                false,
            ),
            (Value::Bool(true), Value::Bool(true), true),
            (Value::Bool(true), Value::Bool(false), false),
            (
                Value::String(String::from("equal")),
                Value::String(String::from("equal")),
                true,
            ),
            (
                Value::String(String::from("equal")),
                Value::String(String::from("not equal")),
                false,
            ),
            (
                Value::Array(vec![
                    Value::new_string("1"),
                    Value::new_string("2"),
                    Value::new_string("3"),
                ]),
                Value::Array(vec![
                    Value::new_string("1"),
                    Value::new_string("2"),
                    Value::new_string("3"),
                ]),
                true,
            ),
            (
                Value::Array(vec![
                    Value::new_string("1"),
                    Value::new_string("2"),
                    Value::new_string("3"),
                ]),
                Value::Array(vec![
                    Value::new_string("3"),
                    Value::new_string("2"),
                    Value::new_string("1"),
                ]),
                false,
            ),
            (
                Value::Array(vec![
                    Value::new_string("1"),
                    Value::new_string("2"),
                    Value::new_string("3"),
                ]),
                Value::Array(vec![
                    Value::new_string("4"),
                    Value::new_string("5"),
                    Value::new_string("6"),
                ]),
                false,
            ),
        ];
        for (lhs, rhs, expected) in cases.iter() {
            let rule = Rule::Function(Function::new_eq(lhs.clone(), rhs.clone()));
            let expected = Some(Value::Bool(*expected));

            assert_eq!(Ok(expected), rule.eval(&input, &mut output));
        }
    }
    #[test]
    fn test_neq_eval() {
        let input = get_default_input();
        let mut output = Output {
            show: true,
            boost: 1.0,
            price: Default::default(),
        };

        let cases = [
            (Value::BigNum(1.into()), Value::BigNum(1.into()), false),
            (Value::BigNum(1.into()), Value::BigNum(2.into()), true),
            (Value::Bool(true), Value::Bool(true), false),
            (Value::Bool(true), Value::Bool(false), true),
            (
                Value::String(String::from("equal")),
                Value::String(String::from("equal")),
                false,
            ),
            (
                Value::String(String::from("equal")),
                Value::String(String::from("not equal")),
                true,
            ),
            (
                Value::Array(vec![
                    Value::new_string("1"),
                    Value::new_string("2"),
                    Value::new_string("3"),
                ]),
                Value::Array(vec![
                    Value::new_string("1"),
                    Value::new_string("2"),
                    Value::new_string("3"),
                ]),
                false,
            ),
            (
                Value::Array(vec![
                    Value::new_string("1"),
                    Value::new_string("2"),
                    Value::new_string("3"),
                ]),
                Value::Array(vec![
                    Value::new_string("4"),
                    Value::new_string("5"),
                    Value::new_string("6"),
                ]),
                true,
            ),
        ];
        for (lhs, rhs, expected) in cases.iter() {
            let rule = Rule::Function(Function::new_neq(lhs.clone(), rhs.clone()));
            let expected = Some(Value::Bool(*expected));

            assert_eq!(Ok(expected), rule.eval(&input, &mut output));
        }
    }
    #[test]
    fn test_only_show_if_eval() {
        let input = get_default_input();
        let mut output = Output {
            show: true,
            boost: 1.0,
            price: Default::default(),
        };
        let result = Function::new_only_show_if(Value::Bool(true)).eval(&input, &mut output);
        assert_eq!(Ok(None), result);
        assert!(output.show);

        let result = Function::new_only_show_if(Value::Bool(false)).eval(&input, &mut output);
        assert_eq!(Ok(None), result);
        assert!(!output.show);
    }
    #[test]
    fn test_do_eval() {
        let input = get_default_input();
        let mut output = Output {
            show: true,
            boost: 1.0,
            price: Default::default(),
        };
        let result = Value::BigNum(200.into());
        let rule = Rule::Function(Function::new_add(
            Value::BigNum(100.into()),
            Value::BigNum(100.into()),
        ));
        let rule_do = Rule::Function(Function::new_do(rule));
        assert_eq!(Ok(Some(result)), rule_do.eval(&input, &mut output));
    }
}

mod string_and_array {
    use super::*;
    #[test]
    fn test_in_eval() {
        let input = get_default_input();
        let mut output = Output {
            show: true,
            boost: 1.0,
            price: Default::default(),
        };

        let cases = vec![
            (
                vec![
                    Value::BigNum(1.into()),
                    Value::BigNum(2.into()),
                    Value::BigNum(3.into()),
                ],
                Value::BigNum(1.into()),
                true,
            ),
            (
                vec![
                    Value::BigNum(1.into()),
                    Value::BigNum(2.into()),
                    Value::BigNum(3.into()),
                ],
                Value::BigNum(0.into()),
                false,
            ),
        ];

        for (arr, value, expected) in cases.into_iter() {
            let rule = Rule::Function(Function::new_in(Value::Array(arr), value));
            let expected = Some(Value::Bool(expected));

            assert_eq!(Ok(expected), rule.eval(&input, &mut output));
        }
    }
    #[test]
    fn test_nin_eval() {
        let input = get_default_input();
        let mut output = Output {
            show: true,
            boost: 1.0,
            price: Default::default(),
        };

        let cases = vec![
            (
                vec![
                    Value::new_number(1),
                    Value::new_number(2),
                    Value::new_number(3),
                ],
                Value::new_number(1),
                false,
            ),
            (
                vec![
                    Value::new_number(1),
                    Value::new_number(2),
                    Value::new_number(3),
                ],
                Value::new_number(0),
                true,
            ),
        ];

        for (arr, value, expected) in cases.into_iter() {
            let rule = Rule::Function(Function::new_nin(Value::Array(arr), value));
            let expected = Some(Value::Bool(expected));

            assert_eq!(Ok(expected), rule.eval(&input, &mut output));
        }
    }
    #[test]
    fn test_at_eval() {
        let input = get_default_input();
        let mut output = Output {
            show: true,
            boost: 1.0,
            price: Default::default(),
        };

        let arr = Value::Array(vec![
            Value::new_number(1),
            Value::new_number(2),
            Value::new_number(3),
        ]);
        let index = Value::new_number(0);
        let at_index = Value::new_number(1);
        let out_of_range = Value::new_number(3);

        let rule = Rule::Function(Function::new_at(arr.clone(), index));
        assert_eq!(Ok(Some(at_index)), rule.eval(&input, &mut output));
        let broken_rule = Rule::Function(Function::new_at(arr.clone(), out_of_range));
        assert_eq!(Err(Error::TypeError), broken_rule.eval(&input, &mut output));
    }
    #[test]
    fn test_split_eval() {
        let input = get_default_input();
        let mut output = Output {
            show: true,
            boost: 1.0,
            price: Default::default(),
        };

        let cases = [
            (
                Value::String(String::from("According to all known laws of aviation, there is no way a bee should be able to fly.")),
                Value::String(String::from(",")),
                Value::Array(vec![Value::String(String::from("According to all known laws of aviation")), Value::String(String::from(" there is no way a bee should be able to fly."))])
            ),
            (
                Value::String(String::from("one two three four five")),
                Value::String(String::from(" ")),
                Value::Array(vec![Value::String(String::from("one")), Value::String(String::from("two")), Value::String(String::from("three")), Value::String(String::from("four")), Value::String(String::from("five"))])
            ),
            (
                Value::String(String::from("broken.spacebar.case")),
                Value::String(String::from(" ")),
                Value::Array(vec![Value::String(String::from("broken.spacebar.case"))])
            )
        ];

        for (string, separator, expected) in cases.iter() {
            let rule = Rule::Function(Function::new_split(
                Rule::Value(string.clone()),
                Rule::Value(separator.clone()),
            ));

            assert_eq!(Ok(Some(expected.clone())), rule.eval(&input, &mut output));
        }
    }
    #[test]
    fn test_starts_with_eval() {
        let input = get_default_input();
        let mut output = Output {
            show: true,
            boost: 1.0,
            price: Default::default(),
        };

        let cases = [
            (
                Value::String(String::from("1234567890")),
                Value::String(String::from("123")),
                true,
            ),
            (
                Value::String(String::from("1234567890")),
                Value::String(String::from("456")),
                false,
            ),
            (
                Value::String(String::from("1234567890")),
                Value::String(String::from("1234567890")),
                true,
            ),
            (
                Value::String(String::from("1234567890")),
                Value::String(String::from("12345678901")),
                false,
            ),
        ];

        for (string, starting, expected) in cases.iter() {
            let rule = Rule::Function(Function::new_starts_with(
                Rule::Value(string.clone()),
                Rule::Value(starting.clone()),
            ));
            let expected = Some(Value::Bool(*expected));
            assert_eq!(Ok(expected), rule.eval(&input, &mut output));
        }
    }
    #[test]
    fn test_ends_with_eval() {
        let input = get_default_input();
        let mut output = Output {
            show: true,
            boost: 1.0,
            price: Default::default(),
        };

        let cases = [
            (
                Value::String(String::from("1234567890")),
                Value::String(String::from("890")),
                true,
            ),
            (
                Value::String(String::from("1234567890")),
                Value::String(String::from("123")),
                false,
            ),
            (
                Value::String(String::from("1234567890")),
                Value::String(String::from("1234567890")),
                true,
            ),
        ];

        for (string, starting, expected) in cases.iter() {
            let rule = Rule::Function(Function::new_ends_with(
                Rule::Value(string.clone()),
                Rule::Value(starting.clone()),
            ));
            let expected = Some(Value::Bool(*expected));
            assert_eq!(Ok(expected), rule.eval(&input, &mut output));
        }
    }

    #[test]
    fn test_get_price_in_usd_eval() {
        let mut output = Output {
            show: true,
            boost: 1.0,
            price: Default::default(),
        };
        for (key, value) in &*DEPOSIT_ASSETS_MAP {
            let mut asset_channel = DUMMY_CHANNEL.clone();
            asset_channel.deposit_asset = key.to_string();
            let input = get_default_input().with_channel(asset_channel);

            let amount_crypto = BigNum::from(100).mul(value);
            let amount_usd = Some(Value::Number(
                Number::from_f64(100.0).expect("should create a float"),
            ));
            let rule = Rule::Function(Function::new_get_price_in_usd(Rule::Value(Value::BigNum(
                amount_crypto,
            ))));
            assert_eq!(Ok(amount_usd), rule.eval(&input, &mut output));
        }
    }
}
