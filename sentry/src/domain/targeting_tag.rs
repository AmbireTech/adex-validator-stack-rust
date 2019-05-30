use serde::{Deserialize, Deserializer, Serialize};

use crate::domain::DomainError;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TargetingTag {
    pub tag: String,
    #[serde(deserialize_with = "score_deserialize")]
    pub score: u8,
//    _secret: (),
}

impl TargetingTag {
    /// score should be between 0 and 100
    pub fn new(tag: String, score: u8) -> Result<Self, DomainError> {
        if score > 100 {
            return Err(DomainError::InvalidArgument("score should be between 0 >= x <= 100".to_string()));
        }

        Ok(Self { tag, score /* _secret: ()*/ })
    }
}

pub fn score_deserialize<'de, D>(deserializer: D) -> Result<u8, D::Error>
    where D: Deserializer<'de>
{
    let score_unchecked: u8 = u8::deserialize(deserializer)?;

    match score_unchecked > 100 {
        true => Err(serde::de::Error::custom("Score should be between 0 >= x <= 100")),
        false => Ok(score_unchecked),
    }
}

#[cfg(test)]
pub(crate) mod fixtures {
    use fake::faker::*;

    use super::TargetingTag;

    pub fn get_targeting_tag(tag: String) -> TargetingTag {
        let score = <Faker as Number>::between(0, 100);

        TargetingTag::new(tag, score).expect("TargetingTag error when creating from fixture")
    }

    pub fn get_targeting_tags(count: usize) -> Vec<TargetingTag> {
        (1..=count)
            .map(|c| {
                let tag_name = format!("tag {}", c);

                get_targeting_tag(tag_name)
            })
            .collect()
    }
}