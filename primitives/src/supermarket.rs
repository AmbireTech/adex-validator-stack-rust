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

        use crate::{
            targeting::{Input, Rules},
            BigNum, ChannelId, ChannelSpec, SpecValidators, ValidatorId, IPFS,
        };
        use chrono::{
            serde::{ts_milliseconds, ts_milliseconds_option},
            DateTime, Utc,
        };
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
            pub price: BigNum,
        }

        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
        #[serde(rename_all = "camelCase")]
        pub struct Campaign {
            #[serde(flatten)]
            pub channel: Channel,
            pub targeting_rules: Rules,
            pub units_with_price: Vec<UnitsWithPrice>,
        }

        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
        #[serde(rename_all = "camelCase")]
        pub struct Channel {
            pub id: ChannelId,
            pub creator: ValidatorId,
            pub deposit_asset: String,
            pub deposit_amount: BigNum,
            pub spec: Spec,
        }

        impl From<crate::Channel> for Channel {
            fn from(channel: crate::Channel) -> Self {
                Self {
                    id: channel.id,
                    creator: channel.creator,
                    deposit_asset: channel.deposit_asset,
                    deposit_amount: channel.deposit_amount,
                    spec: channel.spec.into(),
                }
            }
        }

        #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
        #[serde(rename_all = "camelCase")]
        pub struct Spec {
            #[serde(with = "ts_milliseconds")]
            pub withdraw_period_start: DateTime<Utc>,
            #[serde(
                default,
                skip_serializing_if = "Option::is_none",
                with = "ts_milliseconds_option"
            )]
            pub active_from: Option<DateTime<Utc>>,
            #[serde(with = "ts_milliseconds")]
            pub created: DateTime<Utc>,
            pub validators: SpecValidators,
        }

        impl From<ChannelSpec> for Spec {
            fn from(channel_spec: ChannelSpec) -> Self {
                Self {
                    withdraw_period_start: channel_spec.withdraw_period_start,
                    active_from: channel_spec.active_from,
                    created: channel_spec.created,
                    validators: channel_spec.validators,
                }
            }
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
                    id: ad_unit.ipfs.clone(),
                    media_url: ad_unit.media_url.clone(),
                    media_mime: ad_unit.media_mime.clone(),
                    target_url: ad_unit.target_url.clone(),
                }
            }
        }
    }
}
