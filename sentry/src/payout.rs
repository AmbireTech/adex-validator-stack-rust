use crate::Session;
use chrono::Utc;
use primitives::{
    sentry::Event,
    targeting::Input,
    targeting::{eval_with_callback, get_pricing_bounds, input, Error, Output},
    Address, Campaign, UnifiedNum,
};
use slog::{error, Logger};
use std::cmp::{max, min};

pub type Result = std::result::Result<Option<(Address, UnifiedNum)>, Error>;

/// If None is returned this means that the targeting rules evaluation has set `show = false`
pub fn get_payout(
    logger: &Logger,
    campaign: &Campaign,
    event: &Event,
    session: &Session,
) -> Result {
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
            let targeting_rules = campaign.targeting_rules.clone();

            let pricing = get_pricing_bounds(&campaign, &event_type);

            if targeting_rules.is_empty() {
                Ok(Some((*publisher, pricing.min)))
            } else {
                let ad_unit = ad_unit
                    .as_ref()
                    .and_then(|ipfs| campaign.ad_units.iter().find(|u| &u.ipfs == ipfs));

                let input = Input {
                    ad_view: None,
                    global: input::Global {
                        ad_slot_id: ad_slot.as_ref().map_or(String::new(), ToString::to_string),
                        ad_slot_type: ad_unit.map(|u| u.ad_type.clone()).unwrap_or_default(),
                        publisher_id: *publisher,
                        country: session.country.clone(),
                        event_type: event_type.clone(),
                        seconds_since_epoch: Utc::now(),
                        user_agent_os: session.os.clone(),
                        user_agent_browser_family: None,
                    },
                    ad_unit_id: ad_unit.map(|unit| &unit.ipfs).cloned(),
                    campaign: None,
                    balances: None,
                    ad_slot: None,
                }
                .with_campaign(campaign.clone());

                let mut output = Output {
                    show: true,
                    boost: 1.0,
                    price: vec![(event_type.clone(), pricing.min.clone())]
                        .into_iter()
                        .collect(),
                };

                let on_type_error = |error, rule| error!(logger, "Rule evaluation error for {:?}", campaign.id; "error" => ?error, "rule" => ?rule);

                eval_with_callback(&targeting_rules, &input, &mut output, Some(on_type_error));

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
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use primitives::{
        campaign::{Pricing, PricingBounds},
        util::tests::{
            discard_logger,
            prep_db::{ADDRESSES, DUMMY_CAMPAIGN},
        },
    };

    #[test]
    fn get_event_payouts_pricing_bounds_impression_event() {
        let logger = discard_logger();

        let mut campaign = DUMMY_CAMPAIGN.clone();
        campaign.budget = 100.into();
        campaign.pricing_bounds = Some(PricingBounds {
            impression: Some(Pricing {
                min: 8.into(),
                max: 64.into(),
            }),
            click: Some(Pricing {
                min: 23.into(),
                max: 100.into(),
            }),
        });

        let event = Event::Impression {
            publisher: ADDRESSES["leader"],
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

        let payout = get_payout(&logger, &campaign, &event, &session).expect("Should be OK");

        let expected_option = Some((ADDRESSES["leader"], 8.into()));
        assert_eq!(expected_option, payout, "pricingBounds: impression event");
    }

    #[test]
    fn get_event_payouts_pricing_bounds_click_event() {
        let logger = discard_logger();
        let mut campaign = DUMMY_CAMPAIGN.clone();
        campaign.budget = 100.into();
        campaign.pricing_bounds = Some(PricingBounds {
            impression: Some(Pricing {
                min: 8.into(),
                max: 64.into(),
            }),
            click: Some(Pricing {
                min: 23.into(),
                max: 100.into(),
            }),
        });

        let event = Event::Click {
            publisher: ADDRESSES["leader"],
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

        let payout = get_payout(&logger, &campaign, &event, &session).expect("Should be OK");

        let expected_option = Some((ADDRESSES["leader"], 23.into()));
        assert_eq!(expected_option, payout, "pricingBounds: click event");
    }
}
