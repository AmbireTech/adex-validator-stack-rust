use crate::{
    access::{self, check_access},
    db::{
        accounting::get_accounting_spent,
        campaign::{get_campaigns_by_channel, insert_campaign, update_campaign},
        spendable::fetch_spendable,
        DbPool,
    },
    routes::campaign::update_campaign::set_initial_remaining_for_campaign,
    success_response, Application, Auth, ResponseError, Session,
};
use chrono::Utc;
use deadpool_postgres::PoolError;
use hyper::{Body, Request, Response};
use primitives::{
    adapter::Adapter,
    campaign_validator::Validator,
    sentry::{
        campaign_create::{CreateCampaign, ModifyCampaign},
        Event, SuccessResponse,
    },
    Address, Campaign, CampaignId, UnifiedNum,
};
use redis::{aio::MultiplexedConnection, RedisError};
use slog::error;
use std::{
    cmp::{max, Ordering},
    collections::HashMap,
};
use thiserror::Error;
use tokio_postgres::error::SqlState;

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
    #[error("Spendable amount for campaign creator {0} not found")]
    SpenderNotFound(Address),
    #[error("Campaign was not modified because of spending constraints")]
    CampaignNotModified,
    #[error("Redis error: {0}")]
    Redis(#[from] RedisError),
    #[error("DB Pool error: {0}")]
    Pool(#[from] PoolError),
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

    campaign
        .validate(&app.config, &app.adapter.whoami())
        .map_err(|_| ResponseError::FailedValidation("couldn't valdiate campaign".to_string()))?;

    if auth.uid.to_address() != campaign.creator {
        return Err(ResponseError::Forbidden(
            "Request not sent by campaign creator".to_string(),
        ));
    }

    let error_response =
        ResponseError::BadRequest("err occurred; please try again later".to_string());

    let accounting_spent =
        get_accounting_spent(app.pool.clone(), &campaign.creator, &campaign.channel.id()).await?;

    let latest_spendable =
        fetch_spendable(app.pool.clone(), &campaign.creator, &campaign.channel.id())
            .await?
            .ok_or(ResponseError::BadRequest(
                "No spendable amount found for the Campaign creator".to_string(),
            ))?;
    let total_deposited = latest_spendable.deposit.total;

    let remaining_for_channel =
        total_deposited
            .checked_sub(&accounting_spent)
            .ok_or(ResponseError::FailedValidation(
                "No more budget remaining".to_string(),
            ))?;

    if campaign.budget > remaining_for_channel {
        return Err(ResponseError::BadRequest(
            "Not enough deposit left for the new campaign budget".to_string(),
        ));
    }

    // If the campaign is being created, the amount spent is 0, therefore remaining = budget
    set_initial_remaining_for_campaign(&app.redis, campaign.id, campaign.budget)
        .await
        .map_err(|_| {
            ResponseError::BadRequest(
                "Couldn't update remaining while creating campaign".to_string(),
            )
        })?;

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
        Ok(false) => Err(ResponseError::BadRequest(
            "Encountered error while creating Campaign; please try again".to_string(),
        )),
        _ => Ok(()),
    }?;

    Ok(success_response(serde_json::to_string(&campaign)?))
}

pub mod update_campaign {
    use super::*;

    pub const CAMPAIGN_REMAINING_KEY: &'static str = "campaignRemaining";

    pub async fn set_initial_remaining_for_campaign(
        redis: &MultiplexedConnection,
        id: CampaignId,
        amount: UnifiedNum,
    ) -> Result<bool, Error> {
        let key = format!("{}:{}", CAMPAIGN_REMAINING_KEY, id);
        redis::cmd("SETNX")
            .arg(&key)
            .arg(amount.to_u64())
            .query_async(&mut redis.clone())
            .await?;
        Ok(true)
    }

    pub async fn increase_remaining_for_campaign(
        redis: &MultiplexedConnection,
        id: CampaignId,
        amount: UnifiedNum,
    ) -> Result<UnifiedNum, RedisError> {
        let key = format!("{}:{}", CAMPAIGN_REMAINING_KEY, id);
        redis::cmd("INCRBY")
            .arg(&key)
            .arg(amount.to_u64())
            .query_async::<_, u64>(&mut redis.clone())
            .await
            .map(UnifiedNum::from)
    }

    pub async fn decrease_remaining_for_campaign(
        redis: &MultiplexedConnection,
        id: CampaignId,
        amount: UnifiedNum,
    ) -> Result<i64, RedisError> {
        let key = format!("{}:{}", CAMPAIGN_REMAINING_KEY, id);
        redis::cmd("DECRBY")
            .arg(&key)
            .arg(amount.to_u64())
            .query_async::<_, i64>(&mut redis.clone())
            .await
    }

    pub async fn handle_route<A: Adapter>(
        req: Request<Body>,
        app: &Application<A>,
    ) -> Result<Response<Body>, ResponseError> {
        let campaign_being_mutated = req
            .extensions()
            .get::<Campaign>()
            .expect("We must have a campaign in extensions")
            .to_owned();

        let body = hyper::body::to_bytes(req.into_body()).await?;

        let modify_campaign_fields = serde_json::from_slice::<ModifyCampaign>(&body)
            .map_err(|e| ResponseError::FailedValidation(e.to_string()))?;

        // modify Campaign
        let modified_campaign = modify_campaign(
            &app.pool,
            &mut app.redis.clone(),
            campaign_being_mutated,
            modify_campaign_fields,
        )
        .await
        .map_err(|err| ResponseError::BadRequest(err.to_string()))?;

        Ok(success_response(serde_json::to_string(&modified_campaign)?))
    }

    pub async fn modify_campaign(
        pool: &DbPool,
        redis: &MultiplexedConnection,
        campaign: Campaign,
        modify_campaign: ModifyCampaign,
    ) -> Result<Campaign, Error> {
        // *NOTE*: When updating campaigns make sure sum(campaigns.map(getRemaining)) <= totalDepoisted - totalspent
        // !WARNING!: totalSpent != sum(campaign.map(c => c.spending)) therefore we must always calculate remaining funds based on total_deposit - lastApprovedNewState.spenders[user]
        // *NOTE*: To close a campaign set campaignBudget to campaignSpent so that spendable == 0

        let delta_budget = if let Some(new_budget) = modify_campaign.budget {
            get_delta_budget(redis, &campaign, new_budget).await?
        } else {
            None
        };

        // if we are going to update the budget
        // validate the totalDeposit - totalSpent for all campaign
        // sum(AllChannelCampaigns.map(getRemaining)) + DeltaBudgetForMutatedCampaign <= totalDeposited - totalSpent
        // sum(AllChannelCampaigns.map(getRemaining)) - DeltaBudgetForMutatedCampaign <= totalDeposited - totalSpent
        if let Some(delta_budget) = delta_budget {
            let accounting_spent =
                get_accounting_spent(pool.clone(), &campaign.creator, &campaign.channel.id())
                    .await?;

            let latest_spendable =
                fetch_spendable(pool.clone(), &campaign.creator, &campaign.channel.id())
                    .await?
                    .ok_or(Error::SpenderNotFound(campaign.creator))?;

            // Gets the latest Spendable for this (spender, channelId) pair
            let total_deposited = latest_spendable.deposit.total;

            let total_remaining = total_deposited
                .checked_sub(&accounting_spent)
                .ok_or(Error::Calculation)?;
            let channel_campaigns = get_campaigns_by_channel(&pool, &campaign.channel.id()).await?;

            // this will include the Campaign we are currently modifying
            let campaigns_current_remaining_sum =
                get_remaining_for_multiple_campaigns(&redis, &channel_campaigns)
                    .await?
                    .iter()
                    .sum::<Option<UnifiedNum>>()
                    .ok_or(Error::Calculation)?;

            // apply the delta_budget to the sum
            let new_campaigns_remaining = match delta_budget {
                DeltaBudget::Increase(increase_by) => {
                    campaigns_current_remaining_sum.checked_add(&increase_by)
                }
                DeltaBudget::Decrease(decrease_by) => {
                    campaigns_current_remaining_sum.checked_sub(&decrease_by)
                }
            }
            .ok_or(Error::Calculation)?;

            if !(new_campaigns_remaining <= total_remaining) {
                return Err(Error::CampaignNotModified);
            }

            // if the value is not positive it will return an error because of UnifiedNum
            let _campaign_remaining = match delta_budget {
                // should always be positive
                DeltaBudget::Increase(increase_by) => {
                    increase_remaining_for_campaign(redis, campaign.id, increase_by).await?
                }
                // there is a chance that an even lowered the remaining and it's no longer positive
                // check if positive and create an UnifiedNum, or return an error
                DeltaBudget::Decrease(decrease_by) => {
                    match decrease_remaining_for_campaign(redis, campaign.id, decrease_by).await? {
                        remaining if remaining >= 0 => UnifiedNum::from(remaining.unsigned_abs()),
                        _ => UnifiedNum::from(0),
                    }
                }
            };
        }

        let modified_campaign = modify_campaign.apply(campaign);
        update_campaign(&pool, &modified_campaign).await?;

        Ok(modified_campaign)
    }

    /// Delta Budget describes the difference between the New and Old budget
    /// It is used to decrease or increase the remaining budget instead of setting it up directly
    /// This way if a new event alters the remaining budget in Redis while the modification of campaign hasn't finished
    /// it will correctly update the remaining using an atomic redis operation with `INCRBY` or `DECRBY` instead of using `SET`
    enum DeltaBudget<T> {
        Increase(T),
        Decrease(T),
    }

    async fn get_delta_budget(
        redis: &MultiplexedConnection,
        campaign: &Campaign,
        new_budget: UnifiedNum,
    ) -> Result<Option<DeltaBudget<UnifiedNum>>, Error> {
        let current_budget = campaign.budget;

        let budget_action = match new_budget.cmp(&current_budget) {
            // if there is no difference in budgets - no action needed
            Ordering::Equal => return Ok(None),
            Ordering::Greater => DeltaBudget::Increase(()),
            Ordering::Less => DeltaBudget::Decrease(()),
        };

        let old_remaining = get_remaining_for_campaign(redis, campaign.id)
            .await?
            .ok_or(Error::FailedUpdate(
                "No remaining entry for campaign".to_string(),
            ))?;

        let campaign_spent = campaign
            .budget
            .checked_sub(&old_remaining)
            .ok_or(Error::Calculation)?;

        if campaign_spent >= new_budget {
            return Err(Error::NewBudget(
                "New budget should be greater than the spent amount".to_string(),
            ));
        }

        let budget = match budget_action {
            DeltaBudget::Increase(()) => {
                // delta budget = New budget - Old budget ( the difference between the new and old when New > Old)
                let new_remaining = new_budget
                    .checked_sub(&current_budget)
                    .and_then(|delta_budget| old_remaining.checked_add(&delta_budget))
                    .ok_or(Error::Calculation)?;
                let increase_by = new_remaining
                    .checked_sub(&old_remaining)
                    .ok_or(Error::Calculation)?;

                DeltaBudget::Increase(increase_by)
            }
            DeltaBudget::Decrease(()) => {
                // delta budget = New budget - Old budget ( the difference between the new and old when New > Old)
                let new_remaining = &current_budget
                    .checked_sub(&new_budget)
                    .and_then(|delta_budget| old_remaining.checked_add(&delta_budget))
                    .ok_or(Error::Calculation)?;
                let decrease_by = new_remaining
                    .checked_sub(&old_remaining)
                    .ok_or(Error::Calculation)?;

                DeltaBudget::Decrease(decrease_by)
            }
        };

        Ok(Some(budget))
    }

    pub async fn get_remaining_for_campaign(
        redis: &MultiplexedConnection,
        id: CampaignId,
    ) -> Result<Option<UnifiedNum>, RedisError> {
        let key = format!("{}:{}", CAMPAIGN_REMAINING_KEY, id);

        let remaining = redis::cmd("GET")
            .arg(&key)
            .query_async::<_, Option<i64>>(&mut redis.clone())
            .await?
            .map(|remaining| UnifiedNum::from(max(0, remaining).unsigned_abs()));

        Ok(remaining)
    }

    async fn get_remaining_for_multiple_campaigns(
        redis: &MultiplexedConnection,
        campaigns: &[Campaign],
    ) -> Result<Vec<UnifiedNum>, Error> {
        let keys: Vec<String> = campaigns
            .iter()
            .map(|c| format!("{}:{}", CAMPAIGN_REMAINING_KEY, c.id))
            .collect();

        let remainings = redis::cmd("MGET")
            .arg(keys)
            .query_async::<_, Vec<Option<i64>>>(&mut redis.clone())
            .await?
            .into_iter()
            .map(|remaining| match remaining {
                Some(remaining) => UnifiedNum::from_u64(max(0, remaining).unsigned_abs()),
                None => UnifiedNum::from_u64(0),
            })
            .collect();

        Ok(remainings)
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
    use super::*;
    use crate::{
        campaign::update_campaign::{increase_remaining_for_campaign, CAMPAIGN_REMAINING_KEY},
        db::redis_pool::TESTS_POOL,
    };
    use primitives::util::tests::prep_db::DUMMY_CAMPAIGN;

    #[tokio::test]
    async fn does_it_increase_remaining() {
        let mut redis = TESTS_POOL.get().await.expect("Should return Object");
        let campaign = DUMMY_CAMPAIGN.clone();
        let key = format!("{}:{}", CAMPAIGN_REMAINING_KEY, campaign.id);

        // Setting the redis base variable
        redis::cmd("SET")
            .arg(&key)
            .arg(100_u64)
            .query_async::<_, ()>(&mut redis.connection)
            .await
            .expect("should set");

        // 2 async calls at once, should be 500 after them
        futures::future::try_join_all([
            increase_remaining_for_campaign(&redis, campaign.id, UnifiedNum::from_u64(200)),
            increase_remaining_for_campaign(&redis, campaign.id, UnifiedNum::from_u64(200)),
        ])
        .await
        .expect("Should increase remaining twice");

        let remaining = redis::cmd("GET")
            .arg(&key)
            .query_async::<_, Option<u64>>(&mut redis.connection)
            .await
            .expect("should get remaining");
        assert_eq!(
            remaining.map(UnifiedNum::from_u64),
            Some(UnifiedNum::from_u64(500))
        );

        increase_remaining_for_campaign(&redis, campaign.id, campaign.budget)
            .await
            .expect("should increase");

        let remaining = redis::cmd("GET")
            .arg(&key)
            // Directly parsing to u64 as we know it will be >0
            .query_async::<_, Option<u64>>(&mut redis.connection)
            .await
            .expect("should get remaining");

        let should_be_remaining = UnifiedNum::from_u64(500) + campaign.budget;
        assert_eq!(remaining.map(UnifiedNum::from), Some(should_be_remaining));

        increase_remaining_for_campaign(&redis, campaign.id, UnifiedNum::from_u64(0))
            .await
            .expect("should increase remaining");

        let remaining = redis::cmd("GET")
            .arg(&key)
            .query_async::<_, Option<u64>>(&mut redis.connection)
            .await
            .expect("should get remaining");

        assert_eq!(remaining.map(UnifiedNum::from), Some(should_be_remaining));
    }
}
