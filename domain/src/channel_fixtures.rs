use chrono::{DateTime, Utc};
use fake::faker::*;
use time::Duration;

use crate::asset::fixtures::get_asset;
use crate::fixtures::{get_targeting_tags, get_validator};
use crate::test_util;
use crate::BigNum;

use super::{Channel, ChannelId, ChannelSpec, SpecValidators};

/// It will get the length of channel_id bytes and will fill enough bytes in front
/// If > 32 bytes &str is passed it will `panic!`
pub fn get_channel_id(channel_id: &str) -> ChannelId {
    let channel_id_bytes = channel_id.as_bytes();
    if channel_id_bytes.len() > 32 {
        panic!("The passed &str should be <= 32 bytes");
    }

    let mut id: [u8; 32] = [b'0'; 32];
    for (index, byte) in id[32 - channel_id.len()..].iter_mut().enumerate() {
        *byte = channel_id_bytes[index];
    }

    ChannelId { id }
}

pub fn get_channel(
    id: &str,
    valid_until: &Option<DateTime<Utc>>,
    spec: Option<ChannelSpec>,
) -> Channel {
    let channel_id = get_channel_id(id);
    let deposit_amount = BigNum::from(<Faker as Number>::between(100, 5000));
    let valid_until: DateTime<Utc> = valid_until.unwrap_or_else(|| {
        let future_from = Utc::now() + Duration::days(7);
        test_util::time::datetime_between(&future_from, None)
    });
    let creator = <Faker as Name>::name();
    let deposit_asset = get_asset();
    let spec = spec.unwrap_or_else(|| get_channel_spec(id, None));

    Channel {
        id: channel_id,
        creator,
        deposit_asset,
        deposit_amount,
        valid_until,
        spec,
    }
}

pub fn get_channels(count: usize, valid_until_ge: Option<DateTime<Utc>>) -> Vec<Channel> {
    (1..=count)
        .map(|c| {
            // if we have a valid_until_ge, use it to generate a valid_util for each channel
            let valid_until =
                valid_until_ge.and_then(|ref dt| Some(test_util::time::datetime_between(dt, None)));
            let channel_id = format!("channel {}", c);

            get_channel(&channel_id, &valid_until, None)
        })
        .collect()
}

pub fn get_channel_spec(prefix: &str, validators_option: Option<SpecValidators>) -> ChannelSpec {
    use crate::EventSubmission;
    use test_util::take_one;

    let validators = match validators_option {
        Some(validators) => validators,
        None => [
            get_validator(&format!("{} leader", prefix)),
            get_validator(&format!("{} follower", prefix)),
        ]
        .into(),
    };

    let title_string = Some(<Faker as Lorem>::sentence(3, 4));

    let title = take_one(&[&title_string, &None]).to_owned();
    let max_per_impression = BigNum::from(<Faker as Number>::between(250, 500));
    let min_per_impression = BigNum::from(<Faker as Number>::between(1, 250));
    let nonce = BigNum::from(<Faker as Number>::between(100_000_000, 999_999_999));
    let min_targeting_score =
        take_one(&[&None, &Some(<Faker as Number>::between(1, 500))]).to_owned();

    ChannelSpec {
        validators,
        title,
        max_per_impression,
        min_per_impression,
        targeting: get_targeting_tags(<Faker as Number>::between(0, 5)),
        min_targeting_score,
        // @TODO: `EventSubmission` fixture issue #27
        event_submission: EventSubmission { allow: vec![] },
        created: Utc::now(),
        active_from: Some(Utc::now()),
        nonce,
        withdraw_period_start: Utc::now(),
        ad_units: Vec::new(),
    }
}
