use serde::{Deserialize, Deserializer, Serialize};
use std::error::Error;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TargetingTag {
    pub tag: String,
    pub score: Score,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(transparent)]
pub struct Score(#[serde(deserialize_with = "score_deserialize")] u8);

impl Score {
    /// score should be between 0 and 100
    #[allow(dead_code)]
    fn new(score: u8) -> Result<Self, Box<dyn Error>> {
        if score > 100 {
            return Err("score should be between 0 >= x <= 100".into());
        }

        Ok(Self(score))
    }
}

pub fn score_deserialize<'de, D>(deserializer: D) -> Result<u8, D::Error>
where
    D: Deserializer<'de>,
{
    let score_unchecked: u8 = u8::deserialize(deserializer)?;

    if score_unchecked > 100 {
        Err(serde::de::Error::custom(
            "Score should be between 0 >= x <= 100",
        ))
    } else {
        Ok(score_unchecked)
    }
}

// #[cfg(any(test, feature = "fixtures"))]
// pub mod fixtures {
//     use fake::faker::*;

//     use super::{Score, TargetingTag};

//     pub fn get_targeting_tag(tag: String) -> TargetingTag {
//         TargetingTag {
//             tag,
//             score: get_score(None),
//         }
//     }

//     pub fn get_targeting_tags(count: usize) -> Vec<TargetingTag> {
//         (1..=count)
//             .map(|c| {
//                 let tag_name = format!("tag {}", c);

//                 get_targeting_tag(tag_name)
//             })
//             .collect()
//     }

//     pub fn get_score(score: Option<u8>) -> Score {
//         let score = score.unwrap_or_else(|| <Faker as Number>::between(0, 100));

//         Score::new(score).expect("Score was unable to be created")
//     }
// }
