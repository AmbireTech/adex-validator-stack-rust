use crate::Session;
use chrono::Utc;
use primitives::{
    sentry::Event,
    targeting::{get_pricing_bounds, input, Error, Error as EvalError, Input, Output, Rule, eval_multiple},
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

                let source = input::Source {
                    ad_view: None,
                    global: input::Global {
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
                        channel: Some(channel.clone()),
                        balances: None,
                    },
                    // TODO: Check this one as well!
                    ad_slot: None,
                };
                let input = input::Input::Source(Box::new(source));

                let mut output = Output {
                    show: true,
                    boost: 1.0,
                    price: vec![(event_type.clone(), pricing.min.clone())]
                        .into_iter()
                        .collect(),
                };

                eval_and_log(&targeting_rules, &input, &mut output);

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

fn eval_and_log(/* logger: &Logger, channel_id: ChannelId,  */rules: &[Rule], input: &Input, output: &mut Output) {
    for result in eval_multiple(rules, input, output) {
        match result {
            Ok(_) => {}
            Err(EvalError::UnknownVariable) => {}
            Err(EvalError::TypeError) => {
                todo!();
                // error!(logger, "`WARNING: rule for {:?} failing", channel_id; "rule" => rule, "err" => ?result)
            }
        }

        if !output.show {
            return;
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use primitives::channel::{Pricing, PricingBounds};
    use primitives::util::tests::prep_db::{DUMMY_CHANNEL, IDS};

    #[test]
    fn get_event_payouts_pricing_bounds_impression_event() {
        let mut channel = DUMMY_CHANNEL.clone();
        channel.deposit_amount = 100.into();
        channel.spec.min_per_impression = 8.into();
        channel.spec.max_per_impression = 64.into();
        channel.spec.pricing_bounds = Some(PricingBounds {
            impression: None,
            click: Some(Pricing {
                min: 23.into(),
                max: 100.into(),
            }),
        });

        let event = Event::Impression {
            publisher: IDS["leader"],
            ad_unit: None,
            ad_slot: None,
            referrer: None,
        };

        let session = Session {
            ip: None,
            country: None,
            referrer_header: None,
            os: None,
        };

        let payout = get_payout(&channel, &event, &session).expect("Should be OK");

        let expected_option = Some((IDS["leader"], 8.into()));
        assert_eq!(expected_option, payout, "pricingBounds: impression event");
    }

    #[test]
    fn get_event_payouts_pricing_bounds_click_event() {
        let mut channel = DUMMY_CHANNEL.clone();
        channel.deposit_amount = 100.into();
        channel.spec.min_per_impression = 8.into();
        channel.spec.max_per_impression = 64.into();
        channel.spec.pricing_bounds = Some(PricingBounds {
            impression: None,
            click: Some(Pricing {
                min: 23.into(),
                max: 100.into(),
            }),
        });

        let event = Event::Click {
            publisher: IDS["leader"],
            ad_unit: None,
            ad_slot: None,
            referrer: None,
        };

        let session = Session {
            ip: None,
            country: None,
            referrer_header: None,
            os: None,
        };

        let payout = get_payout(&channel, &event, &session).expect("Should be OK");

        let expected_option = Some((IDS["leader"], 23.into()));
        assert_eq!(expected_option, payout, "pricingBounds: click event");
    }

    #[test]
    fn get_event_payouts_pricing_bounds_close_event() {
        let mut channel = DUMMY_CHANNEL.clone();
        channel.deposit_amount = 100.into();
        channel.spec.min_per_impression = 8.into();
        channel.spec.max_per_impression = 64.into();
        channel.spec.pricing_bounds = Some(PricingBounds {
            impression: None,
            click: Some(Pricing {
                min: 23.into(),
                max: 100.into(),
            }),
        });

        let event = Event::Close;

        let session = Session {
            ip: None,
            country: None,
            referrer_header: None,
            os: None,
        };

        let payout = get_payout(&channel, &event, &session).expect("Should be OK");

        assert_eq!(None, payout, "pricingBounds: click event");
    }
}
