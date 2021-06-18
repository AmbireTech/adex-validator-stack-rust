use crate::{BalancesMap, Channel};

#[derive(Debug, Clone, PartialEq)]
pub struct Campaign {
    pub channel: Channel,
    pub status: Status,
    pub balances: BalancesMap,
}

impl Campaign {
    pub fn new(channel: Channel, status: Status, balances: BalancesMap) -> Self {
        Self {
            channel,
            status,
            balances,
        }
    }
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub enum Status {
    // Active and Ready
    Active,
    Pending,
    Initializing,
    Waiting,
    Finalized(Finalized),
    Unsound {
        disconnected: bool,
        offline: bool,
        rejected_state: bool,
        unhealthy: bool,
    },
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum Finalized {
    Expired,
    Exhausted,
    Withdraw,
}

pub mod units_for_slot {
    pub mod response {

        use crate::{targeting::Input, UnifiedNum, IPFS};
        use serde::{Deserialize, Serialize};
        use url::Url;

        #[derive(Debug, Serialize, Deserialize, PartialEq)]
        #[serde(rename_all = "camelCase")]
        pub struct Response {
            pub targeting_input_base: Input,
            pub accepted_referrers: Vec<Url>,
            pub fallback_unit: Option<AdUnit>,
            pub campaigns: Vec<Campaign>,
        }

        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
        #[serde(rename_all = "camelCase")]
        pub struct UnitsWithPrice {
            pub unit: AdUnit,
            pub price: UnifiedNum,
        }

        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
        #[serde(rename_all = "camelCase")]
        pub struct Campaign {
            #[serde(flatten)]
            pub campaign: crate::Campaign,
            /// Supermarket Specific Campaign field
            pub units_with_price: Vec<UnitsWithPrice>,
        }

        #[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
        #[serde(rename_all = "camelCase")]
        pub struct AdUnit {
            /// Same as `ipfs`
            pub id: IPFS,
            pub media_url: String,
            pub media_mime: String,
            pub target_url: String,
        }

        impl From<&crate::AdUnit> for AdUnit {
            fn from(ad_unit: &crate::AdUnit) -> Self {
                Self {
                    id: ad_unit.ipfs,
                    media_url: ad_unit.media_url.clone(),
                    media_mime: ad_unit.media_mime.clone(),
                    target_url: ad_unit.target_url.clone(),
                }
            }
        }
    }
}
