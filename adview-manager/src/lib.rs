#![deny(rust_2018_idioms)]
#![deny(clippy::all)]

use serde::{Deserialize, Serialize};
use adex_primitives::{AdUnit, TargetingTag, BigNum, SpecValidators};
use adex_primitives::market_channel::{MarketChannel, MarketStatusType};
use chrono::Utc;

pub type TargetingScore = f64;
pub type MinTargetingScore = TargetingScore;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdViewManagerOptions {
    // Defaulted via defaultOpts
    #[serde(rename = "marketURL")]
    pub market_url: String,
    /// Defaulted
    pub accepted_states: Vec<MarketStatusType>,
    /// Defaulted
    pub min_per_impression: BigNum,
    /// Defaulted
    pub min_targeting_score: MinTargetingScore,
    /// Defaulted
    pub randomize: bool,
    pub publisher_addr: String,
    pub whitelisted_token: String,
    pub whitelisted_type: Option<String>,
    /// Defaulted
    pub top_by_price: usize,
    /// Defaulted
    pub top_by_score: usize,
    #[serde(default)]
    pub targeting: Vec<TargetingTag>,
    pub width: Option<u64>,
    pub height: Option<u64>,
    pub fallback_unit: Option<String>,
    /// Defaulted
    pub disabled_video: bool,
}

#[derive(Debug)]
pub struct UnitByPrice {
    pub unit: AdUnit,
    pub channel_id: String,
    pub validators: SpecValidators,
    pub min_targeting_score: MinTargetingScore,
    pub min_per_impression: BigNum,
}

#[derive(Debug)]
pub struct Unit {
    pub unit: AdUnit,
    pub channel_id: String,
    pub validators: SpecValidators,
    pub min_targeting_score: MinTargetingScore,
    pub min_per_impression: BigNum,
    pub targeting_score: TargetingScore,
}

impl Unit {
    pub fn new(by_price: UnitByPrice, targeting_score: TargetingScore) -> Self {
        Self {
            unit: by_price.unit,
            channel_id: by_price.channel_id,
            validators: by_price.validators,
            min_targeting_score: by_price.min_targeting_score,
            min_per_impression: by_price.min_per_impression,
            targeting_score,
        }
    }
}

pub fn apply_selection(campaigns: &[MarketChannel], options: AdViewManagerOptions) -> Vec<Unit> {
    let eligible = campaigns.iter().filter(|campaign| {
        options.accepted_states.contains(&campaign.status.status_type)
        && campaign.spec.active_from.map(|datetime| datetime < Utc::now()).unwrap_or(true)
        && campaign.deposit_asset == options.whitelisted_token
        && campaign.spec.min_per_impression >= options.min_per_impression
    });

    let mut units: Vec<UnitByPrice> = eligible.flat_map(|campaign| {
        let mut units = vec![];
        for ad_unit in campaign.spec.ad_units.iter() {
            let unit = UnitByPrice {
                unit: ad_unit.clone(),
                channel_id: campaign.id.clone(),
                validators: campaign.spec.validators.clone(),
                min_targeting_score: ad_unit.min_targeting_score.or(campaign.spec.min_targeting_score).unwrap_or(0.into()),
                min_per_impression: campaign.spec.min_per_impression.clone(),
            };

            units.push(unit);
        }

        units
    }).collect();

    // Sort
    units.sort_by(|b, a | a.min_per_impression.cmp(&b.min_per_impression));
    units.truncate(options.top_by_price);

    let units = units.into_iter().filter(|unit| {
        options.whitelisted_type.as_ref().map(|whitelisted_type| whitelisted_type != &unit.unit.ad_type && !(options.disabled_video && is_video(&unit.unit))).unwrap_or(false)
    });

    let mut by_score: Vec<Unit> = units.collect::<Vec<UnitByPrice>>().into_iter().filter_map(|by_price| {
        let targeting_score = calculate_target_score(&by_price.unit.targeting, &options.targeting);
        if targeting_score >= options.min_targeting_score && targeting_score >= by_price.min_targeting_score {
            Some(Unit::new(by_price, targeting_score))
        } else {
            None
        }
    }).collect();
    by_score.sort_by(|a, b| a.targeting_score.partial_cmp(&b.targeting_score).expect("Should always be comparable"));
    by_score.truncate(options.top_by_score);

    by_score
}

fn is_video(ad_unit: &AdUnit) -> bool {
    ad_unit.media_mime.split('/').collect::<Vec<&str>>()[0] == "video"
}

fn calculate_target_score(a: &[TargetingTag], b: &[TargetingTag]) -> TargetingScore {
    a.iter().map(|x| -> TargetingScore {
        match b.iter().find(|y| y.tag == x.tag) {
            Some(b) => (&x.score * &b.score).into(),
            None => 0.into(),
        }
    })
        .sum()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Event {
    #[serde( rename = "type")]
    event_type: String,
    publisher: String,
    ad_unit: String,
}

#[derive(Serialize)]
struct EventBody {
    events: Vec<Event>
}

pub fn get_html(options: &AdViewManagerOptions, object: (AdUnit, String, &SpecValidators)) -> String {
    let ev_body = EventBody {
        events: vec![Event { event_type: "IMPRESSION".into(), publisher: options.publisher_addr.clone(), ad_unit: object.0.ipfs.clone()}]
    };

    let on_load_code: String = object.2.into_iter().map(|validator| {
        let fetch_opts = "{ method: 'POST', headers: { 'content-type': 'application/json' }, body: this.dataset.eventBody }";
        let fetch_url = format!("{}/channel/{}/events", validator.url, object.1);

        format!("fetch({}, {});", fetch_url, fetch_opts)
    }).collect();

    on_load_code
}
