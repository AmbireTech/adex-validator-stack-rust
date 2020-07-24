use crate::Session;
use chrono::Utc;
use primitives::{
    sentry::Event,
    targeting::{get_pricing_bounds, Error, Error as EvalError, Global, Input, Output, Rule},
    BigNum, Channel, ValidatorId,
};
use std::{
    cmp::{max, min},
    convert::TryFrom,
};

type Result = std::result::Result<Option<(ValidatorId, BigNum)>, Error>;

pub fn get_payout(channel: &Channel, event: &Event, session: &Session) -> Result {
    let event_type = event.to_string();

    match event {
        Event::Impression {
            publisher,
            ad_unit,
            ad_slot,
            ..
        }
        | Event::Click {
            publisher,
            ad_unit,
            ad_slot,
            ..
        } => {
            /*
            const targetingRules = channel.targetingRules || channel.spec.targetingRules || []
                const eventType = ev.type.toUpperCase()
                const [minPrice, maxPrice] = getPricingBounds(channel, eventType)
                if (targetingRules.length === 0) return [balancesKey, minPrice]

                const adUnit =
                    Array.isArray(channel.spec.adUnits) && channel.spec.adUnits.find(u => u.ipfs === ev.adUnit)
                const targetingInputBase = {*/
            let targeting_rules = if !channel.targeting_rules.is_empty() {
                channel.targeting_rules.clone()
            } else {
                channel.spec.targeting_rules.clone()
            };

            let pricing = get_pricing_bounds(&channel, &event_type);

            if targeting_rules.is_empty() {
                Ok(Some((*publisher, pricing.min)))
            } else {
                let ad_unit = ad_unit
                    .as_ref()
                    .and_then(|ipfs| channel.spec.ad_units.iter().find(|u| &u.ipfs == ipfs));

                let input = Input {
                    ad_view: None,
                    global: Global {
                        // TODO: Check this one!
                        ad_slot_id: ad_slot.clone().unwrap_or_default(),
                        // TODO: Check this one!
                        ad_slot_type: ad_unit.map(|u| u.ad_type.clone()).unwrap_or_default(),
                        publisher_id: *publisher,
                        country: session.country.clone(),
                        event_type: event_type.clone(),
                        // **seconds** means calling `timestamp()`
                        seconds_since_epoch: u64::try_from(Utc::now().timestamp()).expect(
                            "The timestamp (i64) should not overflow or underflow the u64!",
                        ),
                        user_agent_os: session.os.clone(),
                        user_agent_browser_family: None,
                        // TODO: Check this one!
                        ad_unit: ad_unit.cloned(),
                        channel: channel.clone(),
                        status: None,
                        balances: None,
                    },
                    // TODO: Check this one as well!
                    ad_slot: None,
                };

                let mut output = Output {
                    show: true,
                    boost: 1.0,
                    price: vec![(event_type.clone(), pricing.min.clone())]
                        .into_iter()
                        .collect(),
                };

                eval_multiple(&targeting_rules, &input, &mut output);

                if output.show {
                    let price = match output.price.get(&event_type) {
                        Some(output_price) => {
                            max(pricing.min, min(pricing.max, output_price.clone()))
                        }
                        None => max(pricing.min, pricing.max),
                    };

                    Ok(Some((*publisher, price)))
                } else {
                    Ok(None)
                }
            }
        }
        _ => Ok(None),
    }
}

// @TODO: Logging & move to Targeting when ready
fn eval_multiple(rules: &[Rule], input: &Input, output: &mut Output) {
    for rule in rules {
        match rule.eval(input, output) {
            Ok(_) => {}
            Err(EvalError::UnknownVariable) => {}
            Err(EvalError::TypeError) => todo!("OnTypeErr logging"),
        }

        if !output.show {
            return;
        }
    }
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use primitives::channel::{Pricing, PricingBounds};
//     use primitives::util::tests::prep_db::{DUMMY_CHANNEL, IDS};
// }
