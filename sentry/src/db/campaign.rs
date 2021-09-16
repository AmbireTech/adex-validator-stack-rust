use crate::db::{DbPool, PoolError};
use chrono::{DateTime, Utc};
use primitives::{
    sentry::{campaign::CampaignListResponse, Pagination},
    Address, Campaign, CampaignId, ChannelId, ValidatorId,
};
use std::str::FromStr;
use tokio_postgres::types::{accepts, FromSql, Json, ToSql, Type};

pub use campaign_remaining::CampaignRemaining;

/// ```text
/// INSERT INTO campaigns (id, channel_id, channel, creator, budget, validators, title, pricing_bounds, event_submission, ad_units, targeting_rules, created, active_from, active_to)
/// VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
/// ```
pub async fn insert_campaign(pool: &DbPool, campaign: &Campaign) -> Result<bool, PoolError> {
    let client = pool.get().await?;
    let ad_units = Json(campaign.ad_units.clone());
    let stmt = client.prepare("INSERT INTO campaigns (id, channel_id, channel, creator, budget, validators, title, pricing_bounds, event_submission, ad_units, targeting_rules, created, active_from, active_to) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)").await?;
    let inserted = client
        .execute(
            &stmt,
            &[
                &campaign.id,
                &campaign.channel.id(),
                &campaign.channel,
                &campaign.creator,
                &campaign.budget,
                &campaign.validators,
                &campaign.title,
                &campaign.pricing_bounds,
                &campaign.event_submission,
                &ad_units,
                &campaign.targeting_rules,
                &campaign.created,
                &campaign.active.from,
                &campaign.active.to,
            ],
        )
        .await?;

    let inserted = inserted == 1;
    Ok(inserted)
}

/// ```text
/// SELECT id, channel, creator, budget, validators, title, pricing_bounds, event_submission, ad_units, targeting_rules, created, active_from, active_to FROM campaigns
/// WHERE id = $1
/// ```
pub async fn fetch_campaign(
    pool: DbPool,
    campaign: &CampaignId,
) -> Result<Option<Campaign>, PoolError> {
    let client = pool.get().await?;
    let statement = client.prepare("SELECT id, channel, creator, budget, validators, title, pricing_bounds, event_submission, ad_units, targeting_rules, created, active_from, active_to FROM campaigns WHERE id = $1").await?;

    let row = client.query_opt(&statement, &[&campaign]).await?;

    Ok(row.as_ref().map(Campaign::from))
}

// TODO: Export this as it's the same as the one in channel list
struct TotalCount(pub u64);
impl<'a> FromSql<'a> for TotalCount {
    fn from_sql(
        ty: &Type,
        raw: &'a [u8],
    ) -> Result<Self, Box<dyn std::error::Error + Sync + Send>> {
        let str_slice = <&str as FromSql>::from_sql(ty, raw)?;

        Ok(Self(u64::from_str(str_slice)?))
    }

    // Use a varchar or text, since otherwise `int8` fails deserialization
    accepts!(VARCHAR, TEXT);
}

pub async fn list_campaigns(
    pool: &DbPool,
    skip: u64,
    limit: u32,
    creator: &Option<Address>,
    validator: &Option<ValidatorId>,
    is_leader: &Option<bool>,
    active_to_ge: &DateTime<Utc>,
) -> Result<CampaignListResponse, PoolError> {
    let client = pool.get().await?;

    let validator = validator.as_ref().map(|validator_id| {
        serde_json::Value::from_str(&format!(r#"[{{"id": "{}"}}]"#, validator_id))
            .expect("Not a valid json")
    });
    let (where_clauses, params) =
        campaign_list_query_params(creator, validator.as_ref(), is_leader, active_to_ge);
    let total_count_params = (where_clauses.clone(), params.clone());

    // To understand why we use Order by, see Postgres Documentation: https://www.postgresql.org/docs/8.1/queries-limit.html
    let statement = format!("SELECT id, channel, creator, budget, validators, title, pricing_bounds, event_submission, exhausted, ad_units, targeting_rules, created, active_from, active_to FROM campaigns WHERE {} ORDER BY spec->>'created' DESC LIMIT {} OFFSET {}", where_clauses.join(" AND "), limit, skip);
    let stmt = client.prepare(&statement).await?;

    let rows = client.query(&stmt, params.as_slice()).await?;
    let campaigns = rows.iter().map(Campaign::from).collect();

    let total_count =
        list_campaigns_total_count(pool, (&total_count_params.0, total_count_params.1)).await?;

    // fast ceil for total_pages
    let total_pages = if total_count == 0 {
        1
    } else {
        1 + ((total_count - 1) / limit as u64)
    };

    let pagination = Pagination {
        total_pages,
        total: total_pages,
        page: skip / limit as u64,
    };

    Ok(CampaignListResponse {
        pagination,
        campaigns,
    })
}

fn campaign_list_query_params<'a>(
    creator: &'a Option<Address>,
    validator: Option<&'a serde_json::Value>,
    is_leader: &Option<bool>,
    active_to_ge: &'a DateTime<Utc>,
) -> (Vec<String>, Vec<&'a (dyn ToSql + Sync)>) {
    let mut where_clauses = vec!["valid_until >= $1".to_string()];
    let mut params: Vec<&(dyn ToSql + Sync)> = vec![active_to_ge];

    if let Some(creator) = creator {
        where_clauses.push(format!("creator = ${}", params.len() + 1));
        params.push(creator);
    }

    if let Some(validator) = validator {
        where_clauses.push(format!("spec->'validators' @> ${}", params.len() + 1));
        params.push(validator);
    }

    match (validator, is_leader) {
        (Some(validator), Some(true)) => {
            where_clauses.push(format!("channel->'leader' = ${}", params.len() + 1));
            params.push(validator); // TODO: maybe this is redundant
        }
        (None, Some(true)) => {
            // where_clauses.push(format!("channel->'leader' = ${}", params.len() + 1));
            // params.push(authenticated_validator); // TODO
        }
        _ => (),
    }

    (where_clauses, params)
}

async fn list_campaigns_total_count<'a>(
    pool: &DbPool,
    (where_clauses, params): (&'a [String], Vec<&'a (dyn ToSql + Sync)>),
) -> Result<u64, PoolError> {
    let client = pool.get().await?;

    let statement = format!(
        "SELECT COUNT(id)::varchar FROM campaigns WHERE {}",
        where_clauses.join(" AND ")
    );
    let stmt = client.prepare(&statement).await?;
    let row = client.query_one(&stmt, params.as_slice()).await?;

    Ok(row.get::<_, TotalCount>(0).0)
}

// TODO: We might need to use LIMIT to implement pagination
/// ```text
/// SELECT id, channel, creator, budget, validators, title, pricing_bounds, event_submission, ad_units, targeting_rules, created, active_from, active_to
/// FROM campaigns WHERE channel_id = $1
/// ```
pub async fn get_campaigns_by_channel(
    pool: &DbPool,
    channel_id: &ChannelId,
) -> Result<Vec<Campaign>, PoolError> {
    let client = pool.get().await?;
    let statement = client.prepare("SELECT id, channel, creator, budget, validators, title, pricing_bounds, event_submission, ad_units, targeting_rules, created, active_from, active_to FROM campaigns WHERE channel_id = $1").await?;

    let rows = client.query(&statement, &[&channel_id]).await?;

    let campaigns = rows.iter().map(Campaign::from).collect();

    Ok(campaigns)
}

/// ```text
/// UPDATE campaigns SET budget = $1, validators = $2, title = $3, pricing_bounds = $4, event_submission = $5, ad_units = $6, targeting_rules = $7
/// WHERE id = $8
/// RETURNING id, channel, creator, budget, validators, title, pricing_bounds, event_submission, ad_units, targeting_rules, created, active_from, active_to
/// ```
pub async fn update_campaign(pool: &DbPool, campaign: &Campaign) -> Result<Campaign, PoolError> {
    let client = pool.get().await?;
    let statement = client
        .prepare("UPDATE campaigns SET budget = $1, validators = $2, title = $3, pricing_bounds = $4, event_submission = $5, ad_units = $6, targeting_rules = $7 WHERE id = $8 RETURNING id, channel, creator, budget, validators, title, pricing_bounds, event_submission, ad_units, targeting_rules, created, active_from, active_to")
        .await?;

    let ad_units = Json(&campaign.ad_units);

    let updated_row = client
        .query_one(
            &statement,
            &[
                &campaign.budget,
                &campaign.validators,
                &campaign.title,
                &campaign.pricing_bounds,
                &campaign.event_submission,
                &ad_units,
                &campaign.targeting_rules,
                &campaign.id,
            ],
        )
        .await?;

    Ok(Campaign::from(&updated_row))
}

/// struct that handles redis calls for the Campaign Remaining Budget
mod campaign_remaining {
    use crate::db::RedisError;
    use primitives::{CampaignId, UnifiedNum};
    use redis::aio::MultiplexedConnection;

    #[derive(Clone)]
    pub struct CampaignRemaining {
        redis: MultiplexedConnection,
    }

    impl CampaignRemaining {
        pub const CAMPAIGN_REMAINING_KEY: &'static str = "campaignRemaining";

        pub fn get_key(campaign: CampaignId) -> String {
            format!("{}:{}", Self::CAMPAIGN_REMAINING_KEY, campaign)
        }

        pub fn new(redis: MultiplexedConnection) -> Self {
            Self { redis }
        }

        pub async fn set_initial(
            &self,
            campaign: CampaignId,
            amount: UnifiedNum,
        ) -> Result<bool, RedisError> {
            redis::cmd("SETNX")
                .arg(&Self::get_key(campaign))
                .arg(amount.to_u64())
                .query_async(&mut self.redis.clone())
                .await
        }

        pub async fn get_remaining_opt(
            &self,
            campaign: CampaignId,
        ) -> Result<Option<i64>, RedisError> {
            redis::cmd("GET")
                .arg(&Self::get_key(campaign))
                .query_async::<_, Option<i64>>(&mut self.redis.clone())
                .await
        }

        /// This method uses `max(0, value)` to clamp the value of a campaign, which can be negative and uses `i64`.
        /// In addition, it defaults the campaign keys that were not found to `0`.
        pub async fn get_multiple(
            &self,
            campaigns: &[CampaignId],
        ) -> Result<Vec<UnifiedNum>, RedisError> {
            // `MGET` fails on empty keys
            if campaigns.is_empty() {
                return Ok(vec![]);
            }

            let keys: Vec<String> = campaigns
                .iter()
                .map(|campaign| Self::get_key(*campaign))
                .collect();

            let campaigns_remaining = redis::cmd("MGET")
                .arg(keys)
                .query_async::<_, Vec<Option<i64>>>(&mut self.redis.clone())
                .await?
                .into_iter()
                .map(|remaining| match remaining {
                    Some(remaining) => UnifiedNum::from_u64(remaining.max(0).unsigned_abs()),
                    None => UnifiedNum::from_u64(0),
                })
                .collect();

            Ok(campaigns_remaining)
        }

        pub async fn increase_by(
            &self,
            campaign: CampaignId,
            amount: UnifiedNum,
        ) -> Result<i64, RedisError> {
            let key = Self::get_key(campaign);
            redis::cmd("INCRBY")
                .arg(&key)
                .arg(amount.to_u64())
                .query_async(&mut self.redis.clone())
                .await
        }

        pub async fn decrease_by(
            &self,
            campaign: CampaignId,
            amount: UnifiedNum,
        ) -> Result<i64, RedisError> {
            let key = Self::get_key(campaign);
            redis::cmd("DECRBY")
                .arg(&key)
                .arg(amount.to_u64())
                .query_async(&mut self.redis.clone())
                .await
        }
    }

    #[cfg(test)]
    mod test {
        use primitives::util::tests::prep_db::DUMMY_CAMPAIGN;

        use crate::db::redis_pool::TESTS_POOL;

        use super::*;

        #[tokio::test]
        async fn it_sets_initial_increases_and_decreases_remaining_for_campaign() {
            let redis = TESTS_POOL.get().await.expect("Should return Object");

            let campaign = DUMMY_CAMPAIGN.id;
            let campaign_remaining = CampaignRemaining::new(redis.connection.clone());

            // Get remaining on a key which was not set
            {
                let get_remaining = campaign_remaining
                    .get_remaining_opt(campaign)
                    .await
                    .expect("Should get None");

                assert_eq!(None, get_remaining);
            }

            // Set Initial amount on that key
            {
                let initial_amount = UnifiedNum::from(1_000_u64);
                let set_initial = campaign_remaining
                    .set_initial(campaign, initial_amount)
                    .await
                    .expect("Should set value in redis");
                assert!(set_initial);

                // get the remaining of that key, should be the initial value
                let get_remaining = campaign_remaining
                    .get_remaining_opt(campaign)
                    .await
                    .expect("Should get None");

                assert_eq!(
                    Some(1_000_i64),
                    get_remaining,
                    "should return the initial value that was set"
                );
            }

            // Set initial on already existing key, should return `false`
            {
                let set_failing_initial = campaign_remaining
                    .set_initial(campaign, UnifiedNum::from(999_u64))
                    .await
                    .expect("Should set value in redis");
                assert!(!set_failing_initial);
            }

            // Decrease by amount
            {
                let decrease_amount = UnifiedNum::from(64);
                let decrease_by = campaign_remaining
                    .decrease_by(campaign, decrease_amount)
                    .await
                    .expect("Should decrease remaining amount");

                assert_eq!(936_i64, decrease_by);
            }

            // Increase by amount
            {
                let increase_amount = UnifiedNum::from(1064);
                let increase_by = campaign_remaining
                    .increase_by(campaign, increase_amount)
                    .await
                    .expect("Should increase remaining amount");

                assert_eq!(2_000_i64, increase_by);
            }

            let get_remaining = campaign_remaining
                .get_remaining_opt(campaign)
                .await
                .expect("Should get remaining");

            assert_eq!(Some(2_000_i64), get_remaining);

            // Decrease by amount > than currently set
            {
                let decrease_amount = UnifiedNum::from(5_000);
                let decrease_by = campaign_remaining
                    .decrease_by(campaign, decrease_amount)
                    .await
                    .expect("Should decrease remaining amount");

                assert_eq!(-3_000_i64, decrease_by);
            }

            // Increase the negative value without going > 0
            {
                let increase_amount = UnifiedNum::from(1000);
                let increase_by = campaign_remaining
                    .increase_by(campaign, increase_amount)
                    .await
                    .expect("Should increase remaining amount");

                assert_eq!(-2_000_i64, increase_by);
            }
        }

        #[tokio::test]
        async fn it_gets_multiple_campaigns_remaining() {
            let redis = TESTS_POOL.get().await.expect("Should return Object");
            let campaign_remaining = CampaignRemaining::new(redis.connection.clone());

            // get multiple with empty campaigns slice
            // `MGET` throws error on an empty keys argument
            assert!(
                campaign_remaining
                    .get_multiple(&[])
                    .await
                    .expect("Should get multiple")
                    .is_empty(),
                "Should return an empty result"
            );

            let campaigns = (CampaignId::new(), CampaignId::new(), CampaignId::new());

            // set initial amounts
            {
                assert!(campaign_remaining
                    .set_initial(campaigns.0, UnifiedNum::from(100))
                    .await
                    .expect("Should set value in redis"));

                assert!(campaign_remaining
                    .set_initial(campaigns.1, UnifiedNum::from(200))
                    .await
                    .expect("Should set value in redis"));

                assert!(campaign_remaining
                    .set_initial(campaigns.2, UnifiedNum::from(300))
                    .await
                    .expect("Should set value in redis"));
            }

            // set campaigns.1 to negative value, should return `0` because of `max(value, 0)`
            assert_eq!(
                -300_i64,
                campaign_remaining
                    .decrease_by(campaigns.1, UnifiedNum::from(500))
                    .await
                    .expect("Should decrease remaining")
            );

            let multiple = campaign_remaining
                .get_multiple(&[campaigns.0, campaigns.1, campaigns.2])
                .await
                .expect("Should get multiple");

            assert_eq!(
                vec![
                    UnifiedNum::from(100),
                    UnifiedNum::from(0),
                    UnifiedNum::from(300)
                ],
                multiple
            );
        }
    }
}

#[cfg(test)]
mod test {
    use primitives::{
        campaign,
        event_submission::{RateLimit, Rule},
        sentry::campaign_create::ModifyCampaign,
        targeting::Rules,
        util::tests::prep_db::{DUMMY_AD_UNITS, DUMMY_CAMPAIGN},
        EventSubmission, UnifiedNum,
    };
    use std::time::Duration;
    use tokio_postgres::error::SqlState;

    use crate::db::tests_postgres::{setup_test_migrations, DATABASE_POOL};

    use super::*;

    #[tokio::test]
    async fn it_inserts_fetches_and_updates_a_campaign() {
        let database = DATABASE_POOL.get().await.expect("Should get a DB pool");

        setup_test_migrations(database.pool.clone())
            .await
            .expect("Migrations should succeed");

        let campaign = DUMMY_CAMPAIGN.clone();

        let non_existent_campaign = fetch_campaign(database.pool.clone(), &campaign.id)
            .await
            .expect("Should fetch successfully");

        assert_eq!(None, non_existent_campaign);

        let is_inserted = insert_campaign(&database.pool, &campaign)
            .await
            .expect("Should succeed");

        assert!(is_inserted);

        let is_duplicate_inserted = insert_campaign(&database.pool, &campaign).await;

        assert!(matches!(
            is_duplicate_inserted,
            Err(PoolError::Backend(error)) if error.code() == Some(&SqlState::UNIQUE_VIOLATION)
        ));

        let fetched_campaign = fetch_campaign(database.pool.clone(), &campaign.id)
            .await
            .expect("Should fetch successfully");

        assert_eq!(Some(campaign.clone()), fetched_campaign);

        // Update campaign
        {
            let rule = Rule {
                uids: None,
                rate_limit: Some(RateLimit {
                    limit_type: "sid".to_string(),
                    time_frame: Duration::from_millis(20_000),
                }),
            };
            let new_budget = campaign.budget + UnifiedNum::from_u64(1_000_000_000);
            let modified_campaign = ModifyCampaign {
                budget: Some(new_budget),
                validators: None,
                title: Some("Modified Campaign".to_string()),
                pricing_bounds: Some(campaign::PricingBounds {
                    impression: Some(campaign::Pricing {
                        min: 1.into(),
                        max: 10.into(),
                    }),
                    click: Some(campaign::Pricing {
                        min: 0.into(),
                        max: 0.into(),
                    }),
                }),
                event_submission: Some(EventSubmission { allow: vec![rule] }),
                ad_units: Some(DUMMY_AD_UNITS.to_vec()),
                targeting_rules: Some(Rules::new()),
            };

            let applied_campaign = modified_campaign.apply(campaign.clone());

            let updated_campaign = update_campaign(&database.pool, &applied_campaign)
                .await
                .expect("should update");

            assert_eq!(
                applied_campaign, updated_campaign,
                "Postgres should update all modified fields"
            );
        }
    }
}
