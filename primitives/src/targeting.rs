use crate::{campaign::Pricing, sentry::EventType, Campaign, UnifiedNum};

pub use eval::*;
use serde_json::Number;
use std::collections::HashMap;

#[doc(inline)]
pub use input::{field::GetField, Input};

mod eval;
pub mod input;

pub fn get_pricing_bounds(campaign: &Campaign, event_type: &EventType) -> Pricing {
    campaign
        .pricing_bounds
        .get(event_type)
        .cloned()
        .unwrap_or_else(|| Pricing {
            min: 0.into(),
            max: 0.into(),
        })
}

#[derive(Debug)]
pub struct Output {
    /// Whether to show the ad
    /// Default: true
    pub show: bool,
    /// The boost is a number between 0 and 5 that increases the likelyhood for the ad
    /// to be chosen if there is random selection applied on the AdView (multiple ad candidates with the same price)
    /// Default: 1.0
    pub boost: f64,
    /// price.{eventType}
    /// For example: price.IMPRESSION
    /// The default is the min of the bound of event type:
    /// The price is per one event
    /// Default: pricingBounds.IMPRESSION.min
    pub price: HashMap<EventType, UnifiedNum>,
}

impl Output {
    fn try_get(&self, key: &str) -> Result<Value, Error> {
        match key {
            "show" => Ok(Value::Bool(self.show)),
            "boost" => {
                let boost = Number::from_f64(self.boost).ok_or(Error::TypeError)?;
                Ok(Value::Number(boost))
            }
            price_key if price_key.starts_with("price.") => {
                let price = self
                    .price
                    .get(price_key.trim_start_matches("price."))
                    .ok_or(Error::UnknownVariable)?;
                Ok(Value::UnifiedNum(*price))
            }
            _ => Err(Error::UnknownVariable),
        }
    }
}

impl From<&Campaign> for Output {
    fn from(campaign: &Campaign) -> Self {
        let price = campaign
            .pricing_bounds
            .iter()
            .map(|(key, price)| (*key, price.min))
            .collect();

        Self {
            show: true,
            boost: 1.0,
            price,
        }
    }
}

#[cfg(test)]
mod test {
    use crate::sentry::{CLICK, IMPRESSION};

    use super::*;

    #[test]
    fn test_try_get_of_output() {
        let output = Output {
            show: false,
            boost: 5.5,
            price: [(IMPRESSION, 100.into())].into_iter().collect(),
        };

        assert_eq!(Ok(Value::Bool(false)), output.try_get("show"));
        assert_eq!(
            Ok(Value::Number(
                Number::from_f64(5.5).expect("Should make a number")
            )),
            output.try_get("boost")
        );
        assert_eq!(
            Ok(Value::UnifiedNum(100.into())),
            output.try_get("price.IMPRESSION")
        );
        assert_eq!(Err(Error::UnknownVariable), output.try_get("price.unknown"));
        assert_eq!(Err(Error::UnknownVariable), output.try_get("unknown"));
    }

    #[test]
    #[allow(clippy::float_cmp)]
    fn test_output_from_channel() {
        use crate::campaign::Pricing;
        use crate::test_util::DUMMY_CAMPAIGN;

        let mut campaign = DUMMY_CAMPAIGN.clone();
        campaign.pricing_bounds = vec![
            (
                IMPRESSION,
                Pricing {
                    min: 1_000.into(),
                    max: 2_000.into(),
                },
            ),
            (
                CLICK,
                Pricing {
                    min: 3_000.into(),
                    max: 4_000.into(),
                },
            ),
        ]
        .into_iter()
        .collect();

        let output = Output::from(&campaign);

        assert!(output.show);
        assert_eq!(1.0, output.boost);
        assert_eq!(
            Some(&UnifiedNum::from(1_000)),
            output.price.get("IMPRESSION")
        );
        assert_eq!(Some(&UnifiedNum::from(3_000)), output.price.get("CLICK"));
    }
}
