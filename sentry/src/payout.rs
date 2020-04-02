use crate::Session;
use primitives::channel::PriceMultiplicationRules;
use primitives::sentry::Event;
use primitives::ValidatorId;
use primitives::{BigNum, Channel};

pub fn get_payout(channel: &Channel, event: &Event, session: &Session) -> BigNum {
    match event {
        Event::Impression { publisher, .. } | Event::Click { publisher, .. } => {
            let (min, max) = price_bounds(&channel, &event);

            if !channel.spec.price_multiplication_rules.is_empty() {
                payout(
                    &channel.spec.price_multiplication_rules,
                    &event,
                    &session,
                    max,
                    min,
                    &publisher,
                )
            } else {
                min
            }
        }
        _ => Default::default(),
    }
}

fn payout(
    rules: &[PriceMultiplicationRules],
    event: &Event,
    session: &Session,
    max_price: BigNum,
    min_price: BigNum,
    publisher: &ValidatorId,
) -> BigNum {
    let matching_rules: Vec<&PriceMultiplicationRules> = rules
        .iter()
        .filter(|&rule| match_rule(rule, &event, &session, &publisher))
        .collect();
    let fixed_amount_rule = matching_rules
        .iter()
        .find(|&rule| rule.amount.is_some())
        .map(|&rule| rule.amount.as_ref().expect("should have value"));
    let price_by_rules = if let Some(amount) = fixed_amount_rule {
        amount.clone()
    } else {
        let exponent: f64 = 10.0;
        let multiplier = rules
            .iter()
            .filter(|&rule| rule.multiplier.is_some())
            .map(|rule| rule.multiplier.expect("should have value"))
            .fold(1.0, |result, i| result * i);
        let value: u64 = (multiplier * exponent.powi(18)) as u64;
        let result = min_price * BigNum::from(value);

        result / BigNum::from(10u64.pow(18))
    };

    max_price.min(price_by_rules)
}

fn match_rule(
    rule: &PriceMultiplicationRules,
    ev_type: &Event,
    session: &Session,
    uid: &ValidatorId,
) -> bool {
    let ev_type = match &rule.ev_type {
        Some(event_types) => event_types.contains(&ev_type.to_string()),
        None => true,
    };

    let publisher = match &rule.publisher {
        Some(publishers) => publishers.contains(&uid),
        None => true,
    };

    let os_type = match (&rule.os_type, &session.os) {
        (Some(oses), Some(os)) => oses.contains(&os),
        (Some(_), None) => false,
        _ => true,
    };

    let country = match (&rule.country, &session.country) {
        (Some(countries), Some(country)) => countries.contains(&country),
        (Some(_), None) => false,
        _ => true,
    };

    ev_type && publisher && os_type && country
}

fn price_bounds(channel: &Channel, event: &Event) -> (BigNum, BigNum) {
    let pricing_bounds = channel.spec.pricing_bounds.as_ref();
    match (event, pricing_bounds) {
        (Event::Impression { .. }, Some(pricing_bounds)) => {
            match pricing_bounds.impression.as_ref() {
                Some(pricing) => (pricing.min.clone(), pricing.max.clone()),
                _ => (
                    channel.spec.min_per_impression.clone(),
                    channel.spec.max_per_impression.clone(),
                ),
            }
        }
        (Event::Impression { .. }, None) => (
            channel.spec.min_per_impression.clone(),
            channel.spec.max_per_impression.clone(),
        ),
        (Event::Click { .. }, Some(pricing_bounds)) => match pricing_bounds.click.as_ref() {
            Some(pricing) => (pricing.min.clone(), pricing.max.clone()),
            _ => (Default::default(), Default::default()),
        },
        _ => (Default::default(), Default::default()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use primitives::channel::{Pricing, PricingBounds};
    use primitives::util::tests::prep_db::{DUMMY_CHANNEL, IDS};

    #[test]
    fn test_plain_events() {
        let mut channel: Channel = DUMMY_CHANNEL.clone();
        channel.spec.pricing_bounds = Some(PricingBounds {
            click: Some(Pricing {
                min: BigNum::from(23),
                max: BigNum::from(100),
            }),
            impression: None,
        });

        let cases: Vec<(Event, BigNum, String)> = vec![
            (
                Event::Impression {
                    publisher: IDS["publisher"].clone(),
                    ad_slot: None,
                    ad_unit: None,
                    referrer: None,
                },
                BigNum::from(1),
                "pricingBounds: impression event".to_string(),
            ),
            (
                Event::Click {
                    publisher: IDS["publisher"].clone(),
                    ad_slot: None,
                    ad_unit: None,
                    referrer: None,
                },
                BigNum::from(23),
                "pricingBounds: click event".to_string(),
            ),
            (
                Event::Close {},
                BigNum::from(0),
                "pricingBounds: close event".to_string(),
            ),
        ];

        let session = Session {
            ip: None,
            country: None,
            referrer_header: None,
            os: None,
        };

        cases.iter().for_each(|case| {
            let (event, expected_result, message) = case;
            let payout = get_payout(&channel, &event, &session);
            // println!("payout {:?}", payout.to_f64());
            assert!(&payout == expected_result, message.clone());
        })
    }

    #[test]
    fn test_fixed_amount_price_rule_event() {
        let mut channel: Channel = DUMMY_CHANNEL.clone();
        channel.spec.pricing_bounds = Some(PricingBounds {
            click: Some(Pricing {
                min: BigNum::from(23),
                max: BigNum::from(100),
            }),
            impression: None,
        });
        channel.spec.price_multiplication_rules = vec![PriceMultiplicationRules {
            multiplier: None,
            amount: Some(BigNum::from(10)),
            os_type: None,
            ev_type: Some(vec!["CLICK".to_string()]),
            publisher: None,
            country: Some(vec!["us".to_string()]),
        }];

        let cases: Vec<(Event, BigNum, String)> = vec![
            (
                Event::Impression {
                    publisher: IDS["publisher"].clone(),
                    ad_slot: None,
                    ad_unit: None,
                    referrer: None,
                },
                BigNum::from(1),
                "fixedAmount: impression".to_string(),
            ),
            (
                Event::Click {
                    publisher: IDS["publisher"].clone(),
                    ad_slot: None,
                    ad_unit: None,
                    referrer: None,
                },
                BigNum::from(10),
                "fixedAmount (country, publisher): click".to_string(),
            ),
        ];

        let session = Session {
            ip: None,
            country: Some("us".to_string()),
            referrer_header: None,
            os: None,
        };

        cases.iter().for_each(|case| {
            let (event, expected_result, message) = case;
            let payout = get_payout(&channel, &event, &session);
            assert!(&payout == expected_result, message.clone());
        })
    }

    #[test]
    fn test_fixed_amount_exceed_rule_event() {
        let mut channel: Channel = DUMMY_CHANNEL.clone();
        channel.spec.pricing_bounds = Some(PricingBounds {
            click: Some(Pricing {
                min: BigNum::from(23),
                max: BigNum::from(100),
            }),
            impression: None,
        });
        channel.spec.price_multiplication_rules = vec![PriceMultiplicationRules {
            multiplier: None,
            amount: Some(BigNum::from(1000)),
            os_type: None,
            ev_type: None,
            publisher: None,
            country: None,
        }];

        let cases: Vec<(Event, BigNum, String)> = vec![
            (
                Event::Impression {
                    publisher: IDS["publisher"].clone(),
                    ad_slot: None,
                    ad_unit: None,
                    referrer: None,
                },
                BigNum::from(10),
                "fixedAmount (all): price should not exceed maxPerImpressionPrice".to_string(),
            ),
            (
                Event::Click {
                    publisher: IDS["publisher"].clone(),
                    ad_slot: None,
                    ad_unit: None,
                    referrer: None,
                },
                BigNum::from(100),
                "fixedAmount (all): price should not exceed event pricingBound max".to_string(),
            ),
        ];

        let session = Session {
            ip: None,
            country: Some("us".to_string()),
            referrer_header: None,
            os: None,
        };

        cases.iter().for_each(|case| {
            let (event, expected_result, message) = case;
            let payout = get_payout(&channel, &event, &session);
            assert!(&payout == expected_result, message.clone());
        })
    }

    #[test]
    fn test_pick_first_fixed_amount_rule_event() {
        let mut channel: Channel = DUMMY_CHANNEL.clone();
        channel.spec.pricing_bounds = Some(PricingBounds {
            click: Some(Pricing {
                min: BigNum::from(23),
                max: BigNum::from(100),
            }),
            impression: None,
        });
        channel.spec.price_multiplication_rules = vec![
            PriceMultiplicationRules {
                multiplier: None,
                amount: Some(BigNum::from(10)),
                os_type: None,
                ev_type: Some(vec!["CLICK".to_string()]),
                publisher: None,
                country: Some(vec!["us".to_string()]),
            },
            PriceMultiplicationRules {
                multiplier: None,
                amount: Some(BigNum::from(12)),
                os_type: None,
                ev_type: Some(vec!["CLICK".to_string()]),
                publisher: Some(vec![IDS["publisher"].clone()]),
                country: Some(vec!["us".to_string()]),
            },
        ];

        let session = Session {
            ip: None,
            country: Some("us".to_string()),
            referrer_header: None,
            os: None,
        };

        let event = Event::Click {
            publisher: IDS["publisher"].clone(),
            ad_slot: None,
            ad_unit: None,
            referrer: None,
        };

        let payout = get_payout(&channel, &event, &session);
        assert!(
            payout == BigNum::from(10),
            "fixedAmount (country, pulisher): should choose first fixedAmount rule"
        );
    }

    #[test]
    fn test_pick_fixed_amount_rule_over_multiplier_event() {
        let mut channel: Channel = DUMMY_CHANNEL.clone();
        channel.spec.pricing_bounds = Some(PricingBounds {
            click: Some(Pricing {
                min: BigNum::from(23),
                max: BigNum::from(100),
            }),
            impression: None,
        });
        channel.spec.price_multiplication_rules = vec![
            PriceMultiplicationRules {
                multiplier: Some(1.2),
                amount: None,
                os_type: Some(vec!["android".to_string()]),
                ev_type: Some(vec!["CLICK".to_string()]),
                publisher: Some(vec![IDS["publisher"].clone()]),
                country: Some(vec!["us".to_string()]),
            },
            PriceMultiplicationRules {
                multiplier: None,
                amount: Some(BigNum::from(12)),
                os_type: None,
                ev_type: Some(vec!["CLICK".to_string()]),
                publisher: Some(vec![IDS["publisher"].clone()]),
                country: Some(vec!["us".to_string()]),
            },
        ];

        let session = Session {
            ip: None,
            country: Some("us".to_string()),
            referrer_header: None,
            os: None,
        };

        let event = Event::Click {
            publisher: IDS["publisher"].clone(),
            ad_slot: None,
            ad_unit: None,
            referrer: None,
        };

        let payout = get_payout(&channel, &event, &session);
        assert!(
            payout == BigNum::from(12),
            "fixedAmount (country, osType, publisher): choose fixedAmount rule over multiplier if present"
        );
    }

    #[test]
    fn test_apply_all_mutliplier_rules() {
        let mut channel: Channel = DUMMY_CHANNEL.clone();

        channel.spec.pricing_bounds = Some(PricingBounds {
            click: Some(Pricing {
                min: BigNum::from(100),
                max: BigNum::from(1000)
            }),
            impression: None,
        });
        channel.spec.price_multiplication_rules = vec![
            PriceMultiplicationRules {
                multiplier: Some(1.2),
                amount: None,
                os_type: Some(vec!["android".to_string()]),
                ev_type: Some(vec!["CLICK".to_string()]),
                publisher: Some(vec![IDS["publisher"].clone()]),
                country: Some(vec!["us".to_string()]),
            },
            PriceMultiplicationRules {
                multiplier: Some(1.2),
                amount: None,
                os_type: None,
                ev_type: None,
                publisher: None,
                country: None,
            },
        ];

        let session = Session {
            ip: None,
            country: Some("us".to_string()),
            referrer_header: None,
            os: None,
        };

        let event = Event::Click {
            publisher: IDS["publisher"].clone(),
            ad_slot: None,
            ad_unit: None,
            referrer: None,
        };

        let payout = get_payout(&channel, &event, &session);
        assert!(
            payout == BigNum::from(144),
            "fixedAmount (country, osType, publisher): choose fixedAmount rule over multiplier if present"
        );
    }
}
