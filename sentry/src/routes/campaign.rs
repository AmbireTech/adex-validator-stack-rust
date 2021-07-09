use crate::{
    success_response, Application, ResponseError,
    db::{
        spendable::fetch_spendable,
        accounting::get_accounting_spent,
        campaign::{update_campaign, insert_campaign, get_campaigns_by_channel},
        DbPool
    },
    routes::campaign::update_campaign::set_initial_remaining_for_campaign,
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
    campaign_validator::Validator,
    Campaign, CampaignId, UnifiedNum
};
use redis::{
    aio::MultiplexedConnection,
    RedisError,
};
use slog::error;
use std::{
    cmp::{max, Ordering},
    collections::HashMap,
    convert::TryInto,
};
use deadpool_postgres::PoolError;
use tokio_postgres::error::SqlState;

use chrono::Utc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("Error while updating campaign: {0}")]
    FailedUpdate(String),
    #[error("Error while performing calculations")]
    Calculation,
    #[error("Error: Budget has been exceeded")]
    BudgetExceeded,
    #[error("Error with new budget: {0}")]
    NewBudget(String),
    #[error("Redis error: {0}")]
    Redis(#[from] RedisError),
    #[error("DB Pool error: {0}")]
    Pool(#[from] PoolError)
}

pub async fn create_campaign<A: Adapter>(
    req: Request<Body>,
    app: &Application<A>,
) -> Result<Response<Body>, ResponseError> {
    let auth = req
        .extensions()
        .get::<Auth>()
        .expect("request should have session")
        .to_owned();

    let body = hyper::body::to_bytes(req.into_body()).await?;

    let campaign = serde_json::from_slice::<CreateCampaign>(&body)
        .map_err(|e| ResponseError::FailedValidation(e.to_string()))?
        // create the actual `Campaign` with random `CampaignId`
        .into_campaign();

    campaign.validate(&app.config, &app.adapter.whoami()).map_err(|_| ResponseError::FailedValidation("couldn't valdiate campaign".to_string()))?;

    if auth.uid.to_address() != campaign.creator {
        return Err(ResponseError::Forbidden("Request not sent by campaign creator".to_string()))
    }

    let error_response = ResponseError::BadRequest("err occurred; please try again later".to_string());

    let accounting_spent = get_accounting_spent(app.pool.clone(), &campaign.creator, &campaign.channel.id()).await?;

    // TODO: AIP#61: Update when changes to Spendable are ready
    let latest_spendable = fetch_spendable(app.pool.clone(), &campaign.creator, &campaign.channel.id()).await?;
    let total_deposited = latest_spendable.deposit.total;

    let remaining_for_channel = total_deposited.checked_sub(&accounting_spent).ok_or(ResponseError::FailedValidation("No more budget remaining".to_string()))?;

    if campaign.budget > remaining_for_channel {
        return Err(ResponseError::BadRequest("Not enough deposit left for the new campaign budget".to_string()));
    }

    // If the campaign is being created, the amount spent is 0, therefore remaining = budget
    set_initial_remaining_for_campaign(&mut app.redis.clone(), campaign.id, campaign.budget).await.map_err(|_| ResponseError::BadRequest("Couldn't update remaining while creating campaign".to_string()))?;

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

pub mod update_campaign {
    use super::*;

    pub const CAMPAIGN_REMAINING_KEY: &'static str = "campaignRemaining";

    pub async fn set_initial_remaining_for_campaign(redis: &mut MultiplexedConnection, id: CampaignId, amount: UnifiedNum) -> Result<bool, Error> {
        let key = format!("{}:{}", CAMPAIGN_REMAINING_KEY, id);
        redis::cmd("SETNX")
            .arg(&key)
            .arg(amount.to_u64())
            .query_async(redis)
            .await?;
        Ok(true)
    }

    pub async fn increase_remaining_for_campaign(redis: &MultiplexedConnection, id: CampaignId, amount: UnifiedNum) -> Result<bool, Error> {
        let key = format!("{}:{}", CAMPAIGN_REMAINING_KEY, id);
        redis::cmd("INCRBY")
            .arg(&key)
            .arg(amount.to_u64())
            .query_async(&mut redis.clone())
            .await?;
        Ok(true)
    }

    pub async fn decrease_remaining_for_campaign(redis: &MultiplexedConnection, id: CampaignId, amount: UnifiedNum) -> Option<UnifiedNum> {
        let key = format!("{}:{}", CAMPAIGN_REMAINING_KEY, id);
        let value = match redis::cmd("DECRBY")
            .arg(&key)
            .arg(amount.to_u64())
            .query_async::<_, Option<i64>>(&mut redis.clone())
            .await
            {
                Ok(Some(remaining)) => {
                    // Can't be less than 0 due to max()
                    Some(UnifiedNum::from_u64(max(0, remaining).try_into().unwrap()))
                },
                _ => None
            };
        value
    }

    pub async fn handle_route<A: Adapter>(
        req: Request<Body>,
        app: &Application<A>,
    ) -> Result<Response<Body>, ResponseError> {
        let campaign_being_mutated = req.extensions().get::<Campaign>().expect("We must have a campaign in extensions").to_owned();

        let body = hyper::body::to_bytes(req.into_body()).await?;

        let modified_campaign = serde_json::from_slice::<Campaign>(&body)
            .map_err(|e| ResponseError::FailedValidation(e.to_string()))?;

        let modified_campaign = ModifyCampaign::from_campaign(modified_campaign.clone());

        // modify Campaign
        modify_campaign(&app.pool, &campaign_being_mutated, &modified_campaign, &app.redis).await.map_err(|_| ResponseError::BadRequest("Failed to update campaign".to_string()))?;

        Ok(success_response(serde_json::to_string(&modified_campaign)?))
    }

    pub async fn modify_campaign(pool: &DbPool, campaign: &Campaign, modified_campaign: &ModifyCampaign, redis: &MultiplexedConnection) -> Result<Campaign, Error> {
        // *NOTE*: When updating campaigns make sure sum(campaigns.map(getRemaining)) <= totalDepoisted - totalspent
        // !WARNING!: totalSpent != sum(campaign.map(c => c.spending)) therefore we must always calculate remaining funds based on total_deposit - lastApprovedNewState.spenders[user]
        // *NOTE*: To close a campaign set campaignBudget to campaignSpent so that spendable == 0

        if let Some(new_budget) = modified_campaign.budget {
            let old_remaining = get_remaining_for_campaign(&redis, campaign.id).await?.ok_or(Error::FailedUpdate("No remaining entry for campaign".to_string()))?;

            let campaign_spent = campaign.budget.checked_sub(&old_remaining).ok_or(Error::Calculation)?;
            if campaign_spent >= new_budget {
                return Err(Error::NewBudget("New budget should be greater than the spent amount".to_string()));
            }

            // Separate variable for clarity
            let old_budget = campaign.budget;

            match new_budget.cmp(&old_budget) {
                Ordering::Equal => (),
                Ordering::Greater => {
                    let new_remaining = old_remaining.checked_add(&new_budget.checked_sub(&old_budget).ok_or(Error::Calculation)?).ok_or(Error::Calculation)?;
                    let amount_to_incr = new_remaining.checked_sub(&old_remaining).ok_or(Error::Calculation)?;
                    increase_remaining_for_campaign(&redis, campaign.id, amount_to_incr).await?;
                },
                Ordering::Less => {
                    let new_remaining = old_remaining.checked_add(&old_budget.checked_sub(&new_budget).ok_or(Error::Calculation)?).ok_or(Error::Calculation)?;
                    let amount_to_decr = new_remaining.checked_sub(&old_remaining).ok_or(Error::Calculation)?;
                    let decreased_remaining = decrease_remaining_for_campaign(&redis, campaign.id, amount_to_decr).await.ok_or(Error::FailedUpdate("Could't decrease remaining".to_string()))?;
                    // If it goes below 0 it will still return 0
                    if decreased_remaining.eq(&UnifiedNum::from_u64(0)) {
                        return Err(Error::NewBudget("No budget remaining after decreasing".to_string()));
                    }
                }
            }
        };

        let accounting_spent = get_accounting_spent(pool.clone(), &campaign.creator, &campaign.channel.id()).await?;

        let latest_spendable = fetch_spendable(pool.clone(), &campaign.creator, &campaign.channel.id()).await?;

        // Gets the latest Spendable for this (spender, channelId) pair
        let total_deposited = latest_spendable.deposit.total;

        let total_remaining = total_deposited.checked_sub(&accounting_spent).ok_or(Error::Calculation)?;
        let campaigns_for_channel = get_campaigns_by_channel(&pool, &campaign.channel.id()).await?;
        let current_campaign_budget = modified_campaign.budget.unwrap_or(campaign.budget);
        let campaigns_remaining_sum = get_campaigns_remaining_sum(&redis, &campaigns_for_channel, campaign.id, &current_campaign_budget).await.map_err(|_| Error::Calculation)?;
        if campaigns_remaining_sum <= total_remaining {
            let campaign_with_updates = modified_campaign.apply(campaign);
            update_campaign(&pool, &campaign_with_updates).await?;
        }

        Ok(campaign.clone())
    }

    pub async fn get_remaining_for_campaign(redis: &MultiplexedConnection, id: CampaignId) -> Result<Option<UnifiedNum>, RedisError> {
        let key = format!("{}:{}", CAMPAIGN_REMAINING_KEY, id);
        let remaining = match redis::cmd("GET")
            .arg(&key)
            .query_async::<_, Option<i64>>(&mut redis.clone())
            .await {
                Ok(Some(remaining)) => {
                    // Can't be negative due to max()
                    Some(UnifiedNum::from_u64(max(0, remaining).try_into().unwrap()))
                },
                Ok(None) => None,
                Err(e) => return Err(e),
            };
        Ok(remaining)
    }

    async fn get_remaining_for_multiple_campaigns(redis: &MultiplexedConnection, campaigns: &[Campaign]) -> Result<Vec<UnifiedNum>, Error> {
        let keys: Vec<String> = campaigns.iter().map(|c| format!("{}:{}", CAMPAIGN_REMAINING_KEY, c.id)).collect();
        let remainings = redis::cmd("MGET")
            .arg(keys)
            .query_async::<_, Vec<Option<i64>>>(&mut redis.clone())
            .await?;

        let remainings = remainings
            .into_iter()
            .map(|r| {
                match r {
                    // Can't be negative due to max()
                    Some(remaining) => UnifiedNum::from_u64(max(0, remaining).try_into().unwrap()),
                    None => UnifiedNum::from_u64(0)
                }
            })
            .collect();

        Ok(remainings)
    }

    pub async fn get_campaigns_remaining_sum(redis: &MultiplexedConnection, campaigns: &[Campaign], mutated_campaign: CampaignId, new_budget: &UnifiedNum) -> Result<UnifiedNum, Error> {
        let other_campaigns_remaining = get_remaining_for_multiple_campaigns(&redis, &campaigns).await?;
        let sum_of_campaigns_remaining = other_campaigns_remaining
            .iter()
            .sum::<Option<UnifiedNum>>()
            .ok_or(Error::Calculation)?;

        // Necessary to do it explicitly for current campaign as its budget is not yet updated in DB
        let old_remaining_for_mutated_campaign = get_remaining_for_campaign(&redis, mutated_campaign).await?.ok_or(Error::FailedUpdate("No remaining entry for campaign".to_string()))?;
        let spent_for_mutated_campaign = new_budget.checked_sub(&old_remaining_for_mutated_campaign).ok_or(Error::Calculation)?;
        let new_remaining_for_mutated_campaign = new_budget.checked_sub(&spent_for_mutated_campaign).ok_or(Error::Calculation)?;
        sum_of_campaigns_remaining.checked_add(&new_remaining_for_mutated_campaign).ok_or(Error::Calculation)?;
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
        spender::{Deposit, Spendable},
        Address
    };
    use crate::{
        db::redis_pool::TESTS_POOL,
        campaign::update_campaign::{CAMPAIGN_REMAINING_KEY, increase_remaining_for_campaign},
    };
    use super::*;

    // fn get_dummy_spendable(spender: Address, campaign: Campaign) -> Spendable {
    //     Spendable {
    //         spender,
    //         channel: campaign.channel.clone(),
    //         deposit: Deposit {
    //             total: UnifiedNum::from_u64(1_000_000),
    //             still_on_create2: UnifiedNum::from_u64(0),
    //         },
    //     }
    // }

    #[tokio::test]
    async fn does_it_increase_remaining() {
        let mut redis = TESTS_POOL.get().await.expect("Should return Object");
        let campaign = DUMMY_CAMPAIGN.clone();
        let key = format!("{}:{}", CAMPAIGN_REMAINING_KEY, campaign.id);

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
            // Directly parsing to u64 as we know it will be >0
            .query_async::<_, Option<u64>>(&mut redis.connection)
            .await
            .expect("should get remaining");
        assert_eq!(remaining.is_some(), true);

        let remaining = remaining.expect("should get result out of the option");
        let should_be_remaining = UnifiedNum::from_u64(500) + campaign.budget;
        assert_eq!(UnifiedNum::from_u64(remaining), should_be_remaining);

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
}