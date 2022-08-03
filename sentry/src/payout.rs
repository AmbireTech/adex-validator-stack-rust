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

/// If `None` is returned this means that the targeting rules evaluation has set `show = false`
pub fn get_payout(
    logger: &Logger,
    campaign: &Campaign,
    event: &Event,
    session: &Session,
) -> Result {
    let event_type = event.event_type();

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

            let pricing = get_pricing_bounds(campaign, &event_type);

            if targeting_rules.is_empty() {
                Ok(Some((*publisher, pricing.min)))
            } else {
                // Find the event's AdUnit in the Campaign.
                let ad_unit = campaign.ad_units.iter().find(|u| &u.ipfs == ad_unit);

                let input = Input {
                    ad_view: None,
                    global: input::Global {
                        ad_slot_id: *ad_slot,
                        ad_slot_type: ad_unit.map(|u| u.ad_type.clone()).unwrap_or_default(),
                        publisher_id: *publisher,
                        country: session.country.clone(),
                        event_type,
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
                    price: vec![(event_type.to_string(), pricing.min)]
                        .into_iter()
                        .collect(),
                };

                let on_type_error = |error, rule| error!(logger, "Rule evaluation error for {:?}", campaign.id; "error" => ?error, "rule" => ?rule);

                eval_with_callback(&targeting_rules, &input, &mut output, Some(on_type_error));

                if output.show {
                    let price = match output.price.get(event_type.as_str()) {
                        Some(output_price) => max(pricing.min, min(pricing.max, *output_price)),
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
        campaign::Pricing,
        sentry::{CLICK, IMPRESSION},
        test_util::{discard_logger, DUMMY_CAMPAIGN, DUMMY_IPFS, LEADER, PUBLISHER},
    };

    #[test]
    fn get_event_payouts_pricing_bounds_impression_event() {
        let logger = discard_logger();

        let mut campaign = DUMMY_CAMPAIGN.clone();
        campaign.budget = 100.into();
        campaign.pricing_bounds = vec![
            (
                IMPRESSION,
                Pricing {
                    min: 8.into(),
                    max: 64.into(),
                },
            ),
            (
                CLICK,
                Pricing {
                    min: 23.into(),
                    max: 100.into(),
                },
            ),
        ]
        .into_iter()
        .collect();

        let event = Event::Impression {
            publisher: *LEADER,
            ad_unit: DUMMY_IPFS[0],
            ad_slot: DUMMY_IPFS[1],
            referrer: None,
        };

        let session = Session {
            ip: None,
            country: None,
            referrer_header: None,
            os: None,
        };

        let payout = get_payout(&logger, &campaign, &event, &session).expect("Should be OK");

        let expected_option = Some((*LEADER, 8.into()));
        assert_eq!(expected_option, payout, "pricingBounds: impression event");
    }

    #[test]
    fn get_event_payouts_pricing_bounds_click_event() {
        let logger = discard_logger();
        let mut campaign = DUMMY_CAMPAIGN.clone();
        campaign.budget = 100.into();
        campaign.pricing_bounds = vec![
            (
                IMPRESSION,
                Pricing {
                    min: 8.into(),
                    max: 64.into(),
                },
            ),
            (
                CLICK,
                Pricing {
                    min: 23.into(),
                    max: 100.into(),
                },
            ),
        ]
        .into_iter()
        .collect();

        let event = Event::Click {
            publisher: *PUBLISHER,
            ad_unit: DUMMY_IPFS[0],
            ad_slot: DUMMY_IPFS[1],
            referrer: None,
        };

        let session = Session {
            ip: None,
            country: None,
            referrer_header: None,
            os: None,
        };

        let payout = get_payout(&logger, &campaign, &event, &session).expect("Should be OK");

        let expected_option = Some((*PUBLISHER, 23.into()));
        assert_eq!(expected_option, payout, "pricingBounds: click event");
    }
}
