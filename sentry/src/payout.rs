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
    if !rules.is_empty() {
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
    } else {
        min_price
    }
}

fn match_rule(
    rule: &PriceMultiplicationRules,
    ev_type: &Event,
    session: &Session,
    uid: &ValidatorId,
) -> bool {
    let ev_type = match &rule.ev_type {
        Some(event_types) => event_types.contains(ev_type),
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
    match event {
        Event::Impression { .. } => {
            if let Some(pricing_bounds) = pricing_bounds {
                match pricing_bounds.impression.as_ref() {
                    Some(pricing) => (pricing.min.clone(), pricing.max.clone()),
                    _ => (
                        channel.spec.min_per_impression.clone(),
                        channel.spec.max_per_impression.clone(),
                    ),
                }
            } else {
                (
                    channel.spec.min_per_impression.clone(),
                    channel.spec.max_per_impression.clone(),
                )
            }
        }
        Event::Click { .. } => {
            if let Some(pricing_bounds) = pricing_bounds {
                match pricing_bounds.click.as_ref() {
                    Some(pricing) => (pricing.min.clone(), pricing.max.clone()),
                    _ => (Default::default(), Default::default()),
                }
            } else {
                (Default::default(), Default::default())
            }
        }
        _ => (Default::default(), Default::default()),
    }
}

