use crate::{
    channel_v5::Channel, targeting::Rules, AdUnit, Address, EventSubmission, ValidatorDesc,
};

use chrono::{
    serde::{ts_milliseconds, ts_milliseconds_option},
    DateTime, Utc,
};
use serde::{Deserialize, Serialize};
use serde_with::with_prefix;

pub use pricing::{Pricing, PricingBounds};
pub use validators::{ValidatorRole, Validators};

with_prefix!(prefix_active "active_");

#[derive(Debug, Serialize, Deserialize)]
pub struct Campaign {
    pub channel: Channel,
    pub creator: Address,
    pub validators: Validators,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Event pricing bounds
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pricing_bounds: Option<PricingBounds>,
    /// EventSubmission object, applies to event submission (POST /channel/:id/events)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event_submission: Option<EventSubmission>,
    /// An array of AdUnit (optional)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ad_units: Vec<AdUnit>,
    #[serde(default)]
    pub targeting_rules: Rules,
    /// A millisecond timestamp of when the campaign was created
    #[serde(with = "ts_milliseconds")]
    pub created: DateTime<Utc>,
    /// A millisecond timestamp representing the time you want this campaign to become active (optional)
    /// Used by the AdViewManager & Targeting AIP#31
    #[serde(flatten, with = "prefix_active")]
    pub active: Active,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Active {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "ts_milliseconds_option"
    )]
    pub from: Option<DateTime<Utc>>,
    /// A millisecond timestamp of when the campaign should enter a withdraw period
    /// (no longer accept any events other than CHANNEL_CLOSE)
    /// A sane value should be lower than channel.validUntil * 1000 and higher than created
    /// It's recommended to set this at least one month prior to channel.validUntil * 1000
    #[serde(with = "ts_milliseconds")]
    pub active_to: DateTime<Utc>,
}

impl Campaign {
    /// Matches the Channel.leader to the Campaign.spec.leader
    /// If they match it returns `Some`, otherwise, it returns `None`
    pub fn leader<'a>(&'a self) -> Option<&'a ValidatorDesc> {
        if self.channel.leader == self.validators.leader().id {
            Some(self.validators.leader())
        } else {
            None
        }
    }

    /// Matches the Channel.follower to the Campaign.spec.follower
    /// If they match it returns `Some`, otherwise, it returns `None`
    pub fn follower<'a>(&'a self) -> Option<&'a ValidatorDesc> {
        if self.channel.follower == self.validators.follower().id {
            Some(self.validators.follower())
        } else {
            None
        }
    }
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
// TODO: Double check if we require all the methods and enums, as some parts are now in the `Campaign`
// This includes the matching of the Channel leader & follower to the Validators
pub mod validators {
    use crate::{ValidatorDesc, ValidatorId};
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, Debug, Clone, Eq, PartialEq)]
    /// A (leader, follower) tuple
    pub struct Validators(ValidatorDesc, ValidatorDesc);

    #[derive(Debug)]
    pub enum ValidatorRole<'a> {
        Leader(&'a ValidatorDesc),
        Follower(&'a ValidatorDesc),
    }

    impl<'a> ValidatorRole<'a> {
        pub fn validator(&self) -> &'a ValidatorDesc {
            match self {
                ValidatorRole::Leader(validator) => validator,
                ValidatorRole::Follower(validator) => validator,
            }
        }
    }

    impl Validators {
        pub fn new(leader: ValidatorDesc, follower: ValidatorDesc) -> Self {
            Self(leader, follower)
        }

        pub fn leader(&self) -> &ValidatorDesc {
            &self.0
        }

        pub fn follower(&self) -> &ValidatorDesc {
            &self.1
        }

        pub fn find(&self, validator_id: &ValidatorId) -> Option<ValidatorRole<'_>> {
            if &self.leader().id == validator_id {
                Some(ValidatorRole::Leader(&self.leader()))
            } else if &self.follower().id == validator_id {
                Some(ValidatorRole::Follower(&self.follower()))
            } else {
                None
            }
        }

        pub fn find_index(&self, validator_id: &ValidatorId) -> Option<u32> {
            if &self.leader().id == validator_id {
                Some(0)
            } else if &self.follower().id == validator_id {
                Some(1)
            } else {
                None
            }
        }

        pub fn iter(&self) -> Iter<'_> {
            Iter::new(&self)
        }
    }

    impl From<(ValidatorDesc, ValidatorDesc)> for Validators {
        fn from((leader, follower): (ValidatorDesc, ValidatorDesc)) -> Self {
            Self(leader, follower)
        }
    }

    /// Fixed size iterator of 2, as we need an iterator in couple of occasions
    impl<'a> IntoIterator for &'a Validators {
        type Item = &'a ValidatorDesc;
        type IntoIter = Iter<'a>;

        fn into_iter(self) -> Self::IntoIter {
            self.iter()
        }
    }

    pub struct Iter<'a> {
        validators: &'a Validators,
        index: u8,
    }

    impl<'a> Iter<'a> {
        fn new(validators: &'a Validators) -> Self {
            Self {
                validators,
                index: 0,
            }
        }
    }

    impl<'a> Iterator for Iter<'a> {
        type Item = &'a ValidatorDesc;

        fn next(&mut self) -> Option<Self::Item> {
            match self.index {
                0 => {
                    self.index += 1;

                    Some(self.validators.leader())
                }
                1 => {
                    self.index += 1;

                    Some(self.validators.follower())
                }
                _ => None,
            }
        }
    }
}

// TODO: Postgres Campaign
// TODO: Postgres CampaignSpec
// TODO: Postgres Validators
