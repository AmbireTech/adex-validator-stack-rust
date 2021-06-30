use crate::{
    success_response, Application, ResponseError,
    db::{
        spendable::fetch_spendable,
        accounting::get_accounting_spent,
        campaign::{update_campaign, insert_campaign, get_campaigns_by_channel},
        DbPool
    },
    routes::campaign::update_campaign::increase_remaining_for_campaign,
    access::{self, check_access},
    Auth, Session
};

use hyper::{Body, Request, Response};
use primitives::{
    adapter::Adapter,
    sentry::{
        campaign_create::{CreateCampaign, ModifyCampaign},
        Event, SuccessResponse,
    },
    spender::Spendable,
    Campaign, CampaignId, UnifiedNum, BigNum
};
use redis::{
    aio::MultiplexedConnection,
    RedisError,
};
use slog::error;
use std::{
    cmp::max,
    str::FromStr,
    collections::HashMap,
};
use deadpool_postgres::PoolError;
use tokio_postgres::error::SqlState;

use chrono::Utc;

#[derive(Debug, PartialEq, Eq)]
pub enum CampaignError {
    FailedUpdate(String),
    CalculationError,
    BudgetExceeded,
}

impl From<RedisError> for CampaignError {
    fn from(err: RedisError) -> Self {
        CampaignError::FailedUpdate(err.to_string())
    }
}

impl From<PoolError> for CampaignError {
    fn from(err: PoolError) -> Self {
        CampaignError::FailedUpdate(err.to_string())
    }
}

pub async fn create_campaign<A: Adapter>(
    req: Request<Body>,
    app: &Application<A>,
) -> Result<Response<Body>, ResponseError> {
    let body = hyper::body::to_bytes(req.into_body()).await?;

    let campaign = serde_json::from_slice::<CreateCampaign>(&body)
        .map_err(|e| ResponseError::FailedValidation(e.to_string()))?
        // create the actual `Campaign` with random `CampaignId`
        .into_campaign();

    // TODO AIP#61: Validate Campaign

    let error_response = ResponseError::BadRequest("err occurred; please try again later".to_string());

    let accounting_spent = get_accounting_spent(app.pool.clone(), &campaign.creator, &campaign.channel.id()).await?;

    // TODO: AIP#61: Update when changes to Spendable are ready
    let latest_spendable = fetch_spendable(app.pool.clone(), &campaign.creator, &campaign.channel.id()).await?;
    let remaining_for_channel = get_total_remaining_for_channel(&accounting_spent, &latest_spendable).ok_or(ResponseError::BadRequest("couldn't get total remaining for channel".to_string()))?;

    if campaign.budget > remaining_for_channel {
        return Err(ResponseError::BadRequest("Not Enough budget for campaign".to_string()));
    }

    // If the channel is being created, the amount spent is 0, therefore remaining = budget
    increase_remaining_for_campaign(&app.redis.clone(), campaign.id, campaign.budget).await.map_err(|_| ResponseError::BadRequest("Couldn't update remaining while creating campaign".to_string()))?;

    // insert Campaign
    match insert_campaign(&app.pool, &campaign).await {
        Err(error) => {
            error!(&app.logger, "{}", &error; "module" => "create_campaign");
            match error {
                PoolError::Backend(error) if error.code() == Some(&SqlState::UNIQUE_VIOLATION) => {
                    Err(ResponseError::Conflict(
                        "Campaign already exists".to_string(),
                    ))
                }
                _ => Err(error_response),
            }
        }
        Ok(false) => Err(ResponseError::BadRequest("Encountered error while creating Campaign; please try again".to_string())),
        _ => Ok(()),
    }?;

    Ok(success_response(serde_json::to_string(&campaign)?))
}


// tested
fn get_total_remaining_for_channel(accounting_spent: &UnifiedNum, latest_spendable: &Spendable) -> Option<UnifiedNum> {
    let total_deposited = latest_spendable.deposit.total;

    let total_remaining = total_deposited.checked_sub(&accounting_spent);
    total_remaining
}

pub mod update_campaign {
    use super::*;
    use lazy_static::lazy_static;

    lazy_static!{
        pub static ref CAMPAIGN_REMAINING_KEY: &'static str = "campaignRemaining";
    }

    pub async fn increase_remaining_for_campaign(redis: &MultiplexedConnection, id: CampaignId, amount: UnifiedNum) -> Result<bool, CampaignError> {
        let key = format!("{}:{}", *CAMPAIGN_REMAINING_KEY, id);
        redis::cmd("INCRBY")
            .arg(&key)
            .arg(amount.to_u64())
            .query_async(&mut redis.clone())
            .await?;
        Ok(true)
    }

    pub async fn decrease_remaining_for_campaign(redis: &MultiplexedConnection, id: CampaignId, amount: UnifiedNum) -> Result<bool, CampaignError> {
        let key = format!("{}:{}", *CAMPAIGN_REMAINING_KEY, id);
        redis::cmd("DECRBY")
            .arg(&key)
            .arg(amount.to_u64())
            .query_async(&mut redis.clone())
            .await?;
        Ok(true)
    }

    pub async fn handle_route<A: Adapter>(
        req: Request<Body>,
        app: &Application<A>,
    ) -> Result<Response<Body>, ResponseError> {
        let campaign = req.extensions().get::<Campaign>().expect("We must have a campaign in extensions");

        let modified_campaign = ModifyCampaign::from_campaign(campaign.clone());

        // modify Campaign
        modify_campaign(&app.pool, &campaign, &modified_campaign, &app.redis).await.map_err(|_| ResponseError::BadRequest("Failed to update campaign".to_string()))?;

        Ok(success_response(serde_json::to_string(&campaign)?))
    }

    pub async fn modify_campaign(pool: &DbPool, campaign: &Campaign, modified_campaign: &ModifyCampaign, redis: &MultiplexedConnection) -> Result<Campaign, CampaignError> {
        // *NOTE*: When updating campaigns make sure sum(campaigns.map(getRemaining)) <= totalDepoisted - totalspent
        // !WARNING!: totalSpent != sum(campaign.map(c => c.spending)) therefore we must always calculate remaining funds based on total_deposit - lastApprovedNewState.spenders[user]
        // *NOTE*: To close a campaign set campaignBudget to campaignSpent so that spendable == 0

        let new_budget = modified_campaign.budget.ok_or(CampaignError::FailedUpdate("Couldn't get new budget".to_string()))?;
        let accounting_spent = get_accounting_spent(pool.clone(), &campaign.creator, &campaign.channel.id()).await?;

        let latest_spendable = fetch_spendable(pool.clone(), &campaign.creator, &campaign.channel.id()).await?;

        let old_remaining = get_remaining_for_campaign_from_redis(&redis, campaign.id).await.ok_or(CampaignError::FailedUpdate("Couldn't get remaining for campaign".to_string()))?;

        let campaign_spent = new_budget.checked_sub(&old_remaining).ok_or(CampaignError::CalculationError)?;
        if campaign_spent >= new_budget {
            return Err(CampaignError::BudgetExceeded);
        }

        let old_remaining = UnifiedNum::from_u64(max(0, old_remaining.to_u64()));

        let new_remaining = new_budget.checked_sub(&campaign_spent).ok_or(CampaignError::CalculationError)?;

        if new_remaining >= old_remaining {
            let diff_in_remaining = new_remaining.checked_sub(&old_remaining).ok_or(CampaignError::CalculationError)?;
            increase_remaining_for_campaign(&redis, campaign.id, diff_in_remaining).await?;
        } else {
            let diff_in_remaining = old_remaining.checked_sub(&new_remaining).ok_or(CampaignError::CalculationError)?;
            decrease_remaining_for_campaign(&redis, campaign.id, diff_in_remaining).await?;
        }


        // Gets the latest Spendable for this (spender, channelId) pair
        let total_remaining = get_total_remaining_for_channel(&accounting_spent, &latest_spendable).ok_or(CampaignError::FailedUpdate("Could not get total remaining for channel".to_string()))?;
        let campaigns_for_channel = get_campaigns_by_channel(&pool, &campaign.channel.id()).await?;
        let campaigns_remaining_sum = get_campaigns_remaining_sum(&redis, &campaigns_for_channel, campaign.id, &new_budget).await.map_err(|_| CampaignError::CalculationError)?;
        if campaigns_remaining_sum > total_remaining {
            return Err(CampaignError::BudgetExceeded);
        }

        update_campaign(&pool, &campaign).await?;

        Ok(campaign.clone())
    }

    // TODO: #382 Remove after #412 is merged
    fn get_unified_num_from_string(value: &str) -> Option<UnifiedNum> {
        let value_as_big_num: Option<BigNum> = BigNum::from_str(value).ok();
        let value_as_u64 = match value_as_big_num {
            Some(num) => num.to_u64(),
            _ => None,
        };
        let value_as_unified = match value_as_u64 {
            Some(num) => Some(UnifiedNum::from_u64(num)),
            _ => None
        };
        value_as_unified
    }

    pub async fn get_remaining_for_campaign_from_redis(redis: &MultiplexedConnection, id: CampaignId) -> Option<UnifiedNum> {
        let key = format!("{}:{}", *CAMPAIGN_REMAINING_KEY, id);
        let remaining = match redis::cmd("GET")
            .arg(&key)
            .query_async::<_, Option<String>>(&mut redis.clone())
            .await
            {
                Ok(Some(remaining)) => {
                    // TODO: #382 Just parse from string once #412 is merged
                    get_unified_num_from_string(&remaining)
                },
                _ => None
            };
        remaining
    }

    async fn get_remaining_for_multiple_campaigns(redis: &MultiplexedConnection, campaigns: &[Campaign]) -> Result<Vec<UnifiedNum>, CampaignError> {
        let keys: Vec<String> = campaigns.into_iter().map(|c| format!("{}:{}", *CAMPAIGN_REMAINING_KEY, c.id)).collect();
        let remainings = redis::cmd("MGET")
            .arg(keys)
            .query_async::<_, Vec<Option<String>>>(&mut redis.clone())
            .await?;

        let remainings = remainings
            .into_iter()
            .flat_map(|r| r)
            .map(|r| get_unified_num_from_string(&r))
            .flatten()
            .collect();

        Ok(remainings)
    }

    pub async fn get_campaigns_remaining_sum(redis: &MultiplexedConnection, campaigns: &[Campaign], mutated_campaign: CampaignId, new_budget: &UnifiedNum) -> Result<UnifiedNum, CampaignError> {
        let other_campaigns_remaining = get_remaining_for_multiple_campaigns(&redis, &campaigns).await?;
        let sum_of_campaigns_remaining = other_campaigns_remaining
            .into_iter()
            .try_fold(UnifiedNum::from_u64(0), |sum, val| sum.checked_add(&val).ok_or(CampaignError::CalculationError))?;

        // Necessary to do it explicitly for current campaign as its budget is not yet updated in DB
        let old_remaining_for_mutated_campaign = get_remaining_for_campaign_from_redis(&redis, mutated_campaign).await.ok_or(CampaignError::CalculationError)?;
        let spent_for_mutated_campaign = new_budget.checked_sub(&old_remaining_for_mutated_campaign).ok_or(CampaignError::CalculationError)?;
        let new_remaining_for_mutated_campaign = new_budget.checked_sub(&spent_for_mutated_campaign).ok_or(CampaignError::CalculationError)?;
        sum_of_campaigns_remaining.checked_add(&new_remaining_for_mutated_campaign).ok_or(CampaignError::CalculationError)?;
        Ok(sum_of_campaigns_remaining)
    }
}

pub async fn insert_events<A: Adapter + 'static>(
    req: Request<Body>,
    app: &Application<A>,
) -> Result<Response<Body>, ResponseError> {
    let (req_head, req_body) = req.into_parts();

    let auth = req_head.extensions.get::<Auth>();
    let session = req_head
        .extensions
        .get::<Session>()
        .expect("request should have session");

    let campaign = req_head
        .extensions
        .get::<Campaign>()
        .expect("request should have a Campaign loaded");

    let body_bytes = hyper::body::to_bytes(req_body).await?;
    let mut request_body = serde_json::from_slice::<HashMap<String, Vec<Event>>>(&body_bytes)?;

    let events = request_body
        .remove("events")
        .ok_or_else(|| ResponseError::BadRequest("invalid request".to_string()))?;

    let processed = process_events(app, auth, session, campaign, events).await?;

    Ok(Response::builder()
        .header("Content-type", "application/json")
        .body(serde_json::to_string(&SuccessResponse { success: processed })?.into())
        .unwrap())
}

async fn process_events<A: Adapter + 'static>(
    app: &Application<A>,
    auth: Option<&Auth>,
    session: &Session,
    campaign: &Campaign,
    events: Vec<Event>,
) -> Result<bool, ResponseError> {
    if &Utc::now() > &campaign.active.to {
        return Err(ResponseError::BadRequest("Campaign is expired".into()));
    }

    //
    // TODO #381: AIP#61 Spender Aggregator should be called
    //

    // handle events - check access
    // handle events - Update targeting rules
    // calculate payout
    // distribute fees
    // handle spending - Spender Aggregate
    // handle events - aggregate Events and put into analytics

    check_access(
        &app.redis,
        session,
        auth,
        &app.config.ip_rate_limit,
        &campaign,
        &events,
    )
    .await
    .map_err(|e| match e {
        access::Error::ForbiddenReferrer => ResponseError::Forbidden(e.to_string()),
        access::Error::RulesError(error) => ResponseError::TooManyRequests(error),
        access::Error::UnAuthenticated => ResponseError::Unauthorized,
        _ => ResponseError::BadRequest(e.to_string()),
    })?;

    Ok(true)
}

#[cfg(test)]
mod test {
    use primitives::{
        util::tests::prep_db::{DUMMY_CAMPAIGN},
        spender::Deposit,
        Address
    };
    use deadpool::managed::Object;
    use crate::{
        db::redis_pool::{Manager, TESTS_POOL},
        campaign::update_campaign::CAMPAIGN_REMAINING_KEY,
    };
    use std::convert::TryFrom;
    use super::*;

    async fn get_redis() -> Object<Manager> {
        let connection = TESTS_POOL.get().await.expect("Should return Object");
        connection
    }

    fn get_campaign() -> Campaign {
        DUMMY_CAMPAIGN.clone()
    }

    fn get_dummy_spendable(spender: Address) -> Spendable {
        Spendable {
            spender,
            channel: DUMMY_CAMPAIGN.channel.clone(),
            deposit: Deposit {
                total: UnifiedNum::from_u64(1_000_000),
                still_on_create2: UnifiedNum::from_u64(0),
            },
        }
    }

    #[tokio::test]
    async fn does_it_get_total_remaianing() {
        let campaign = get_campaign();
        let accounting_spent = UnifiedNum::from_u64(100_000);
        let latest_spendable = get_dummy_spendable(campaign.creator);

        let total_remaining = get_total_remaining_for_channel(&accounting_spent, &latest_spendable).expect("should calculate");

        assert_eq!(total_remaining, UnifiedNum::from_u64(900_000));
    }

    #[tokio::test]
    async fn does_it_increase_remaining() {
        let mut redis = get_redis().await;
        let campaign = get_campaign();
        let key = format!("{}:{}", *CAMPAIGN_REMAINING_KEY, campaign.id);

        // Setting the redis base variable
        redis::cmd("SET")
            .arg(&key)
            .arg(100u64)
            .query_async::<_, ()>(&mut redis.connection)
            .await
            .expect("should set");

        // 2 async calls at once, should be 500 after them
        futures::future::join(
            increase_remaining_for_campaign(&redis, campaign.id, UnifiedNum::from_u64(200)),
            increase_remaining_for_campaign(&redis, campaign.id, UnifiedNum::from_u64(200))
        ).await;

        let remaining = redis::cmd("GET")
            .arg(&key)
            .query_async::<_, Option<String>>(&mut redis.connection)
            .await
            .expect("should get remaining");
        assert_eq!(remaining.is_some(), true);

        let remaining = remaining.expect("should get remaining");
        let remaining = UnifiedNum::from_u64(remaining.parse::<u64>().expect("should parse"));
        assert_eq!(remaining, UnifiedNum::from_u64(500));

        increase_remaining_for_campaign(&redis, campaign.id, campaign.budget).await.expect("should increase");

        let remaining = redis::cmd("GET")
            .arg(&key)
            .query_async::<_, Option<String>>(&mut redis.connection)
            .await
            .expect("should get remaining");
        assert_eq!(remaining.is_some(), true);

        let remaining = remaining.expect("should get remaining");
        let remaining = UnifiedNum::from_u64(remaining.parse::<u64>().expect("should parse"));
        let should_be_remaining = UnifiedNum::from_u64(500).checked_add(&campaign.budget).expect("should add");
        assert_eq!(remaining, should_be_remaining);

        increase_remaining_for_campaign(&redis, campaign.id, UnifiedNum::from_u64(0)).await.expect("should work");

        let remaining = redis::cmd("GET")
            .arg(&key)
            .query_async::<_, Option<String>>(&mut redis.connection)
            .await
            .expect("should get remaining");
        assert_eq!(remaining.is_some(), true);

        let remaining = remaining.expect("should get remaining");
        let remaining = UnifiedNum::from_u64(remaining.parse::<u64>().expect("should parse"));
        assert_eq!(remaining, should_be_remaining);
    }

    #[tokio::test]
    async fn update_remaining_before_it_is_set() {
        let mut redis = get_redis().await;
        let campaign = get_campaign();
        let key = format!("{}:{}", *CAMPAIGN_REMAINING_KEY, campaign.id);

        let remaining = redis::cmd("GET")
            .arg(&key)
            .query_async::<_, Option<String>>(&mut redis.connection)
            .await
            .expect("should return None");

        assert_eq!(remaining, None)
    }

    // test get_campaigns_remaining_sum

    // test get_remaining_for_multiple_campaigns

    // test get_remaining_for_campaign_from_redis
}