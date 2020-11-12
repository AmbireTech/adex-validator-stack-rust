use crate::{channel::Pricing, BigNum, Channel};

pub use eval::*;
use serde_json::Number;
use std::collections::HashMap;

pub use input::{field::GetField, Input};

mod eval;
pub mod input;

pub fn get_pricing_bounds(channel: &Channel, event_type: &str) -> Pricing {
    channel
        .spec
        .pricing_bounds
        .as_ref()
        .and_then(|pricing_bounds| pricing_bounds.get(event_type))
        .cloned()
        .unwrap_or_else(|| {
            if event_type == "IMPRESSION" {
                Pricing {
                    min: channel.spec.min_per_impression.clone().max(1.into()),
                    max: channel.spec.max_per_impression.clone().max(1.into()),
                }
            } else {
                Pricing {
                    min: 0.into(),
                    max: 0.into(),
                }
            }
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
    /// Default: pricingBounds.IMPRESSION.min
    pub price: HashMap<String, BigNum>,
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
                Ok(Value::BigNum(price.clone()))
            }
            _ => Err(Error::UnknownVariable),
        }
    }
}

impl From<&Channel> for Output {
    fn from(channel: &Channel) -> Self {
        let price = match &channel.spec.pricing_bounds {
            Some(pricing_bounds) => pricing_bounds
                .to_vec()
                .into_iter()
                .map(|(key, price)| (key.to_string(), price.min))
                .collect(),
            _ => Default::default(),
        };

        Self {
            show: true,
            boost: 1.0,
            price,
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_try_get_of_output() {
        let output = Output {
            show: false,
            boost: 5.5,
            price: vec![("one".to_string(), 100.into())].into_iter().collect(),
        };

        assert_eq!(Ok(Value::Bool(false)), output.try_get("show"));
        assert_eq!(
            Ok(Value::Number(
                Number::from_f64(5.5).expect("Should make a number")
            )),
            output.try_get("boost")
        );
        assert_eq!(Ok(Value::BigNum(100.into())), output.try_get("price.one"));
        assert_eq!(Err(Error::UnknownVariable), output.try_get("price.unknown"));
        assert_eq!(Err(Error::UnknownVariable), output.try_get("unknown"));
    }

    #[test]
    fn test_output_from_channel() {
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

        let output = Output::from(&channel);

        assert_eq!(true, output.show);
        assert_eq!(1.0, output.boost);
        assert_eq!(Some(&BigNum::from(1_000)), output.price.get("IMPRESSION"));
        assert_eq!(Some(&BigNum::from(3_000)), output.price.get("CLICK"));
    }
}
