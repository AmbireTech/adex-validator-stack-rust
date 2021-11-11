use crate::db::{DbPool, PoolError, TotalCount};
use chrono::{DateTime, Utc};
use primitives::{
    sentry::{
        campaign::{CampaignListResponse, ValidatorParam},
        Pagination,
    },
    Address, Campaign, CampaignId, ChannelId,
};
use tokio_postgres::types::{Json, ToSql};

pub use campaign_remaining::CampaignRemaining;

/// ```text
/// INSERT INTO campaigns (id, channel_id, creator, budget, validators, title, pricing_bounds, event_submission, ad_units, targeting_rules, created, active_from, active_to)
/// VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
/// ```
pub async fn insert_campaign(pool: &DbPool, campaign: &Campaign) -> Result<bool, PoolError> {
    let client = pool.get().await?;
    let ad_units = Json(campaign.ad_units.clone());
    let stmt = client.prepare("INSERT INTO campaigns (id, channel_id, creator, budget, validators, title, pricing_bounds, event_submission, ad_units, targeting_rules, created, active_from, active_to) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)").await?;
    let inserted = client
        .execute(
            &stmt,
            &[
                &campaign.id,
                &campaign.channel.id(),
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
/// SELECT campaigns.id, creator, budget, validators, title, pricing_bounds, event_submission, ad_units, targeting_rules, campaigns.created, active_from, active_to,
/// channels.leader, channels.follower, channels.guardian, channels.token, channels.nonce
/// FROM campaigns INNER JOIN channels
/// ON campaigns.channel_id=channels.id
/// WHERE campaigns.id = $1
/// ```
pub async fn fetch_campaign(
    pool: DbPool,
    campaign: &CampaignId,
) -> Result<Option<Campaign>, PoolError> {
    let client = pool.get().await?;
    // TODO: Check and update
    let statement = client.prepare("SELECT campaigns.id, creator, budget, validators, title, pricing_bounds, event_submission, ad_units, targeting_rules, campaigns.created, active_from, active_to, channels.leader, channels.follower, channels.guardian, channels.token, channels.nonce FROM campaigns INNER JOIN channels
    ON campaigns.channel_id=channels.id WHERE campaigns.id = $1").await?;

    let row = client.query_opt(&statement, &[&campaign]).await?;

    Ok(row.as_ref().map(Campaign::from))
}

pub async fn list_campaigns(
    pool: &DbPool,
    skip: u64,
    limit: u32,
    creator: Option<Address>,
    validator: Option<ValidatorParam>,
    active_to_ge: &DateTime<Utc>,
) -> Result<CampaignListResponse, PoolError> {
    let client = pool.get().await?;

    let (where_clauses, params) = campaign_list_query_params(&creator, &validator, active_to_ge);
    let total_count_params = (where_clauses.clone(), params.clone());

    // To understand why we use Order by, see Postgres Documentation: https://www.postgresql.org/docs/8.1/queries-limit.html
    let statement = format!("SELECT campaigns.id, creator, budget, validators, title, pricing_bounds, event_submission, ad_units, targeting_rules, campaigns.created, active_from, active_to, channels.leader, channels.follower, channels.guardian, channels.token, channels.nonce FROM campaigns INNER JOIN channels ON campaigns.channel_id=channels.id WHERE {} ORDER BY campaigns.created ASC LIMIT {} OFFSET {}", where_clauses.join(" AND "), limit, skip);
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
        page: skip / limit as u64,
    };

    Ok(CampaignListResponse {
        pagination,
        campaigns,
    })
}

fn campaign_list_query_params<'a>(
    creator: &'a Option<Address>,
    validator: &'a Option<ValidatorParam>,
    active_to_ge: &'a DateTime<Utc>,
) -> (Vec<String>, Vec<&'a (dyn ToSql + Sync)>) {
    let mut where_clauses = vec!["active_to >= $1".to_string()];
    let mut params: Vec<&(dyn ToSql + Sync)> = vec![active_to_ge];

    if let Some(creator) = creator {
        where_clauses.push(format!("creator = ${}", params.len() + 1));
        params.push(creator);
    }

    // if clause for is_leader is true, the other clause is also always true
    match validator {
        Some(ValidatorParam::Leader(validator_id)) => {
            where_clauses.push(format!("channels.leader = ${}", params.len() + 1));
            params.push(validator_id);
        }
        Some(ValidatorParam::Validator(validator_id)) => {
            where_clauses.push(format!(
                "(channels.leader = ${x} OR channels.follower = ${x})",
                x = params.len() + 1,
            ));
            params.push(validator_id);
        }
        _ => (),
    }

    (where_clauses, params)
}

pub async fn list_campaigns_total_count<'a>(
    pool: &DbPool,
    (where_clauses, params): (&'a [String], Vec<&'a (dyn ToSql + Sync)>),
) -> Result<u64, PoolError> {
    let client = pool.get().await?;

    let statement = format!(
        "SELECT COUNT(campaigns.id)::varchar FROM campaigns INNER JOIN channels ON campaigns.channel_id=channels.id WHERE {}",
        where_clauses.join(" AND ")
    );
    let stmt = client.prepare(&statement).await?;
    let row = client.query_one(&stmt, params.as_slice()).await?;

    Ok(row.get::<_, TotalCount>(0).0)
}

/// ```text
/// SELECT id FROM campaigns WHERE channel_id = $1 ORDER BY created ASC LIMIT {} OFFSET {}
/// ```
pub async fn get_campaign_ids_by_channel(
    pool: &DbPool,
    channel_id: &ChannelId,
    limit: u64,
    skip: u64,
) -> Result<Vec<CampaignId>, PoolError> {
    let client = pool.get().await?;

    let query = format!(
        "SELECT id FROM campaigns WHERE channel_id = $1 ORDER BY created ASC LIMIT {} OFFSET {}",
        limit, skip
    );

    let statement = client.prepare(&query).await?;

    let rows = client.query(&statement, &[&channel_id]).await?;
    let campaign_ids = rows.iter().map(CampaignId::from).collect();

    Ok(campaign_ids)
}

/// ```text
/// UPDATE campaigns SET budget = $1, validators = $2, title = $3, pricing_bounds = $4, event_submission = $5, ad_units = $6, targeting_rules = $7
/// FROM channels WHERE campaigns.id = $8 AND campaigns.channel_id=channels.id
/// RETURNING campaigns.id, creator, budget, validators, title, pricing_bounds, event_submission, ad_units, targeting_rules, campaigns.created, active_from, active_to,
/// channels.leader, channels.follower, channels.guardian, channels.token, channels.nonce
/// ```
pub async fn update_campaign(pool: &DbPool, campaign: &Campaign) -> Result<Campaign, PoolError> {
    let client = pool.get().await?;
    let statement = client
        .prepare("UPDATE campaigns SET budget = $1, validators = $2, title = $3, pricing_bounds = $4, event_submission = $5, ad_units = $6, targeting_rules = $7 FROM channels WHERE campaigns.id = $8 AND campaigns.channel_id=channels.id RETURNING campaigns.id, creator, budget, validators, title, pricing_bounds, event_submission, ad_units, targeting_rules, campaigns.created, active_from, active_to, channels.leader, channels.follower, channels.guardian, channels.token, channels.nonce")
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

        /// Doesn't allow the usage of SET with a predefined amount due to a possible race condition
        /// use increase/decrease functions instead
        pub async fn set_remaining_to_zero(&self, campaign: CampaignId) -> Result<bool, RedisError> {
            let key = CampaignRemaining::get_key(campaign);

            redis::cmd("SET")
                .arg(&key)
                .arg(0)
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
    use crate::db::{
        insert_channel,
        tests_postgres::{setup_test_migrations, DATABASE_POOL},
    };
    use chrono::TimeZone;
    use primitives::{
        campaign,
        campaign::Validators,
        event_submission::{RateLimit, Rule},
        sentry::campaign_create::ModifyCampaign,
        targeting::Rules,
        util::tests::prep_db::{
            ADDRESSES, DUMMY_AD_UNITS, DUMMY_CAMPAIGN, DUMMY_VALIDATOR_FOLLOWER, IDS,
        },
        EventSubmission, UnifiedNum, ValidatorDesc, ValidatorId,
    };
    use std::time::Duration;
    use tokio_postgres::error::SqlState;

    use super::*;

    #[tokio::test]
    async fn it_inserts_fetches_and_updates_a_campaign() {
        let database = DATABASE_POOL.get().await.expect("Should get a DB pool");

        setup_test_migrations(database.pool.clone())
            .await
            .expect("Migrations should succeed");

        let campaign = DUMMY_CAMPAIGN.clone();

        // insert the channel into the DB
        let _channel = insert_channel(&database.pool, DUMMY_CAMPAIGN.channel)
            .await
            .expect("Should insert");

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

    // Campaigns are sorted in ascending order when retrieved
    // Therefore the last campaign inserted will come up first in results
    #[tokio::test]
    async fn it_lists_campaigns_properly() {
        let database = DATABASE_POOL.get().await.expect("Should get a DB pool");

        setup_test_migrations(database.pool.clone())
            .await
            .expect("Migrations should succeed");

        let campaign = DUMMY_CAMPAIGN.clone();
        let mut channel_with_different_leader = DUMMY_CAMPAIGN.channel;
        channel_with_different_leader.leader = IDS["user"];

        insert_channel(&database, DUMMY_CAMPAIGN.channel)
            .await
            .expect("Should insert");
        insert_channel(&database, channel_with_different_leader)
            .await
            .expect("Should insert");

        let mut campaign_new_id = DUMMY_CAMPAIGN.clone();
        campaign_new_id.id = CampaignId::new();
        campaign_new_id.created = Utc.ymd(2020, 2, 1).and_hms(7, 0, 0); // 1 year before previous

        // campaign with a different creator
        let mut campaign_new_creator = DUMMY_CAMPAIGN.clone();
        campaign_new_creator.id = CampaignId::new();
        campaign_new_creator.creator = ADDRESSES["tester"];
        campaign_new_creator.created = Utc.ymd(2019, 2, 1).and_hms(7, 0, 0); // 1 year before previous

        let mut campaign_new_leader = DUMMY_CAMPAIGN.clone();
        campaign_new_leader.id = CampaignId::new();
        campaign_new_leader.created = Utc.ymd(2018, 2, 1).and_hms(7, 0, 0); // 1 year before previous

        let different_leader: ValidatorDesc = ValidatorDesc {
            id: ValidatorId::try_from("0x20754168c00a6e58116ccfd0a5f7d1bb66c5de9d")
                .expect("Failed to parse DUMMY_VALIDATOR_DIFFERENT_LEADER id"),
            url: "http://localhost:8005".to_string(),
            fee: 100.into(),
            fee_addr: None,
        };
        campaign_new_leader.channel = channel_with_different_leader;
        campaign_new_leader.validators =
            Validators::new((different_leader.clone(), DUMMY_VALIDATOR_FOLLOWER.clone()));

        insert_campaign(&database, &campaign)
            .await
            .expect("Should insert"); // fourth
        insert_campaign(&database, &campaign_new_id)
            .await
            .expect("Should insert"); // third
        insert_campaign(&database, &campaign_new_creator)
            .await
            .expect("Should insert"); // second
        insert_campaign(&database, &campaign_new_leader)
            .await
            .expect("Should insert"); // first

        // 2 out of 3 results
        let first_page = list_campaigns(
            &database.pool,
            0,
            2,
            Some(ADDRESSES["creator"]),
            None,
            &DUMMY_CAMPAIGN.created,
        )
        .await
        .expect("should fetch");
        assert_eq!(
            first_page.campaigns,
            vec![campaign_new_leader.clone(), campaign_new_id.clone()]
        );

        // 3rd result
        let second_page = list_campaigns(
            &database.pool,
            2,
            2,
            Some(ADDRESSES["creator"]),
            None,
            &DUMMY_CAMPAIGN.created,
        )
        .await
        .expect("should fetch");
        assert_eq!(second_page.campaigns, vec![campaign.clone()]);

        // No results past limit
        let third_page = list_campaigns(
            &database.pool,
            4,
            2,
            Some(ADDRESSES["creator"]),
            None,
            &DUMMY_CAMPAIGN.created,
        )
        .await
        .expect("should fetch");
        assert_eq!(third_page.campaigns.len(), 0);

        // Test with a different creator
        let first_page = list_campaigns(
            &database.pool,
            0,
            2,
            Some(ADDRESSES["tester"]),
            None,
            &DUMMY_CAMPAIGN.created,
        )
        .await
        .expect("should fetch");
        assert_eq!(first_page.campaigns, vec![campaign_new_creator.clone()]);

        // Test with validator
        let first_page = list_campaigns(
            &database.pool,
            0,
            5,
            None,
            Some(ValidatorParam::Validator(IDS["follower"])),
            &DUMMY_CAMPAIGN.created,
        )
        .await
        .expect("should fetch");
        assert_eq!(
            first_page.campaigns,
            vec![
                campaign_new_leader.clone(),
                campaign_new_creator.clone(),
                campaign_new_id.clone(),
                campaign.clone()
            ]
        );

        // Test with leader validator
        let first_page = list_campaigns(
            &database.pool,
            0,
            5,
            None,
            Some(ValidatorParam::Leader(IDS["leader"])),
            &DUMMY_CAMPAIGN.created,
        )
        .await
        .expect("should fetch");
        assert_eq!(
            first_page.campaigns,
            vec![
                campaign_new_creator.clone(),
                campaign_new_id.clone(),
                campaign.clone()
            ]
        );

        // Test with a different leader validator
        let first_page = list_campaigns(
            &database.pool,
            0,
            5,
            None,
            Some(ValidatorParam::Leader(IDS["user"])),
            &DUMMY_CAMPAIGN.created,
        )
        .await
        .expect("should fetch");
        assert_eq!(first_page.campaigns, vec![campaign_new_leader.clone()]);

        // Test with leader validator but validator isn't the leader of any campaign
        let first_page = list_campaigns(
            &database.pool,
            0,
            5,
            None,
            Some(ValidatorParam::Leader(IDS["follower"])),
            &DUMMY_CAMPAIGN.created,
        )
        .await
        .expect("should fetch");
        assert_eq!(first_page.campaigns.len(), 0);

        // Test with creator and provided leader validator
        let first_page = list_campaigns(
            &database.pool,
            0,
            5,
            Some(ADDRESSES["creator"]),
            Some(ValidatorParam::Leader(IDS["leader"])),
            &DUMMY_CAMPAIGN.created,
        )
        .await
        .expect("should fetch");
        assert_eq!(
            first_page.campaigns,
            vec![campaign_new_id.clone(), campaign.clone()]
        );
    }
}
