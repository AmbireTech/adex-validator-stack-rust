use crate::{channel_v5::Channel, targeting::Rules, AdUnit, BigNum, EventSubmission, SpecValidators};

use chrono::{
    serde::{ts_milliseconds, ts_milliseconds_option},
    DateTime, Utc,
};
use serde::{Deserialize, Serialize};

pub use pricing::{Pricing, PricingBounds};

#[derive(Debug, Serialize, Deserialize)]
pub struct Campaign {
    channel: Channel,
    spec: CampaignSpec,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CampaignSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub validators: SpecValidators,
    /// Event pricing bounds
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pricing_bounds: Option<PricingBounds>,
    /// EventSubmission object, applies to event submission (POST /channel/:id/events)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_submission: Option<EventSubmission>,
    /// A millisecond timestamp of when the campaign was created
    #[serde(with = "ts_milliseconds")]
    pub created: DateTime<Utc>,
    /// A millisecond timestamp representing the time you want this campaign to become active (optional)
    /// Used by the AdViewManager & Targeting AIP#31
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "ts_milliseconds_option"
    )]
    pub active_from: Option<DateTime<Utc>>,
    /// A random number to ensure the campaignSpec hash is unique
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nonce: Option<BigNum>,
    /// A millisecond timestamp of when the campaign should enter a withdraw period
    /// (no longer accept any events other than CHANNEL_CLOSE)
    /// A sane value should be lower than channel.validUntil * 1000 and higher than created
    /// It's recommended to set this at least one month prior to channel.validUntil * 1000
    #[serde(with = "ts_milliseconds")]
    pub withdraw_period_start: DateTime<Utc>,
    /// An array of AdUnit (optional)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ad_units: Vec<AdUnit>,
    #[serde(default)]
    pub targeting_rules: Rules,
}

mod pricing {
    use crate::BigNum;
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
    pub struct Pricing {
        pub max: BigNum,
        pub min: BigNum,
    }

    #[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
    #[serde(rename_all = "UPPERCASE")]
    pub struct PricingBounds {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub impression: Option<Pricing>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pub click: Option<Pricing>,
    }

    impl PricingBounds {
        pub fn to_vec(&self) -> Vec<(&str, Pricing)> {
            let mut vec = Vec::new();

            if let Some(pricing) = self.impression.as_ref() {
                vec.push(("IMPRESSION", pricing.clone()));
            }

            if let Some(pricing) = self.click.as_ref() {
                vec.push(("CLICK", pricing.clone()))
            }

            vec
        }

        pub fn get(&self, event_type: &str) -> Option<&Pricing> {
            match event_type {
                "IMPRESSION" => self.impression.as_ref(),
                "CLICK" => self.click.as_ref(),
                _ => None,
            }
        }
    }
}
// TODO: Move SpecValidators (spec::Validators?)

// TODO: Postgres Campaign
// TODO: Postgres CampaignSpec
