use crate::{
    success_response, Application, ResponseError,
    db::{
        spendable::fetch_spendable,
        accounting::get_accounting_spent,
        DbPool
    },
};
use hyper::{Body, Request, Response};
use primitives::{
    adapter::Adapter,
    sentry::{
        campaign_create::CreateCampaign,
    },
    spender::Spendable,
    Campaign, CampaignId, UnifiedNum, BigNum
};
use redis::aio::MultiplexedConnection;
use slog::error;
use crate::db::campaign::{update_campaign as update_campaign_db, insert_campaign, get_campaigns_for_channel};
use std::{
    cmp::max,
    str::FromStr,
};
use futures::future::join_all;
use std::collections::HashMap;

use crate::{
    access::{self, check_access},
    success_response, Application, Auth, ResponseError, Session,
};
use chrono::Utc;
use hyper::{Body, Request, Response};
use primitives::{
    adapter::Adapter,
    sentry::{campaign_create::CreateCampaign, Event, SuccessResponse},
    Campaign,
};
use crate::routes::campaign::modify_campaign::{get_remaining_for_campaign_from_redis, get_campaigns_remaining_sum};

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
    let remaining_for_channel = get_total_remaining_for_channel(&accounting_spent, &latest_spendable)?;

    if campaign.budget > remaining_for_channel {
        return Err(ResponseError::Conflict("Not Enough budget for campaign".to_string()));
    }

    // If the channel is being created, the amount spent is 0, therefore remaining = budget
    update_remaining_for_campaign(&app.redis.clone(), campaign.id, campaign.budget).await?;

    // insert Campaign
    match insert_campaign(&app.pool, &campaign).await {
        Err(error) => {
            error!(&app.logger, "{}", &error; "module" => "create_campaign");
            return Err(ResponseError::Conflict("campaign already exists".to_string()));
        }
        Ok(false) => Err(error_response),
        _ => Ok(()),
    }?;

    Ok(success_response(serde_json::to_string(&campaign)?))
}

pub async fn update_campaign<A: Adapter>(
    req: Request<Body>,
    app: &Application<A>,
) -> Result<Response<Body>, ResponseError> {
    let body = hyper::body::to_bytes(req.into_body()).await?;

    let campaign = serde_json::from_slice::<Campaign>(&body)
        .map_err(|e| ResponseError::FailedValidation(e.to_string()))?;

    let error_response = ResponseError::BadRequest("err occurred; please try again later".to_string());

    // modify Campaign

    match modify_campaign(&app.pool, &campaign, &app.redis).await {
        Err(error) => {
            error!(&app.logger, "{:?}", &error; "module" => "update_campaign");
            return Err(ResponseError::Conflict("Error modifying campaign".to_string()));
        }
        Ok(false) => Err(error_response),
        _ => Ok(()),
    }?;

    Ok(success_response(serde_json::to_string(&campaign)?))
}

pub async fn modify_campaign(pool: &DbPool, campaign: &Campaign, redis: &MultiplexedConnection) -> Result<bool, ResponseError> {
    let accounting_spent = get_accounting_spent(pool.clone(), &campaign.creator, &campaign.channel.id()).await?;

    let latest_spendable = fetch_spendable(pool.clone(), &campaign.creator, &campaign.channel.id()).await?;

    let old_remaining = get_remaining_for_campaign_from_redis(&redis, campaign.id).await?;
    let campaign_spent = campaign.budget.checked_sub(&old_remaining)?;

    let old_remaining = UnifiedNum::from_u64(max(0, old_remaining.to_u64()));

    let new_remaining = campaign.budget.checked_sub(&campaign_spent).ok_or_else(|| {
        ResponseError::Conflict("Error while subtracting campaign_spent from budget".to_string())
    })?;

    let diff_in_remaining = new_remaining.checked_sub(&old_remaining).ok_or_else(|| {
        ResponseError::Conflict("Error while subtracting campaign_spent from budget".to_string())
    })?;

    update_remaining_for_campaign(&redis, campaign.id, diff_in_remaining).await?;

    // Gets the latest Spendable for this (spender, channelId) pair
    let total_remaining = get_total_remaining_for_channel(&accounting_spent, &latest_spendable)?;
    let campaigns_for_channel = get_campaigns_for_channel(&pool, &campaign).await?;
    let campaigns_remaining_sum = get_campaigns_remaining_sum(&redis, &campaigns_for_channel, &campaign).await?;
    if campaigns_remaining_sum > total_remaining {
        return Err(ResponseError::Conflict("Remaining for campaigns exceeds total remaining for channel".to_string()));
    }

    update_campaign_db(&pool, &campaign).await.map_err(|e| ResponseError::Conflict(e.to_string()))

    // *NOTE*: When updating campaigns make sure sum(campaigns.map(getRemaining)) <= totalDepoisted - totalspent
    // !WARNING!: totalSpent != sum(campaign.map(c => c.spending)) therefore we must always calculate remaining funds based on total_deposit - lastApprovedNewState.spenders[user]
    // *NOTE*: To close a campaign set campaignBudget to campaignSpent so that spendable == 0
}

// tested
async fn update_remaining_for_campaign(redis: &MultiplexedConnection, id: CampaignId, amount: UnifiedNum) -> Result<bool, ResponseError> {
    // update a key in Redis for the remaining spendable amount
    let key = format!("remaining:{}", id);
    redis::cmd("INCRBY")
        .arg(&key)
        .arg(amount.to_u64())
        .query_async(&mut redis.clone())
        .await
        .map_err(|_| ResponseError::Conflict("Error updating remainingSpendable for current campaign".to_string()))?;
    Ok(true)
}

// tested
fn get_total_remaining_for_channel(accounting_spent: &UnifiedNum, latest_spendable: &Spendable) -> Option<UnifiedNum> {
    let total_deposited = latest_spendable.deposit.total;

    let total_remaining = total_deposited.checked_sub(&accounting_spent);
    total_remaining
}

mod modify_campaign {
    use super::*;
    pub async fn get_remaining_for_campaign_from_redis(redis: &MultiplexedConnection, id: CampaignId) -> Result<UnifiedNum, ResponseError> {
        let key = format!("remaining:{}", id);
        let remaining = match redis::cmd("GET")
            .arg(&key)
            .query_async::<_, Option<String>>(&mut redis.clone())
            .await
            {
                Ok(Some(remaining)) => {
                    let res = BigNum::from_str(&remaining)?;
                    let res = res.to_u64().ok_or_else(|| {
                        ResponseError::Conflict("Error while calculating the total remaining amount".to_string())
                    })?;
                    Ok(UnifiedNum::from_u64(res))
                },
                _ => Ok(UnifiedNum::from_u64(0))
            };
        remaining
    }

    async fn get_remaining_for_multiple_campaigns(redis: &MultiplexedConnection, campaigns: &[Campaign], mutated_campaign_id: CampaignId) -> Result<Vec<UnifiedNum>, ResponseError> {
        let other_campaigns_remaining = campaigns
            .into_iter()
            .filter(|c| c.id != mutated_campaign_id)
            .map(|c| async move {
                let remaining = get_remaining_for_campaign_from_redis(&redis, c.id).await?;
                Ok(remaining)
            })
            .collect::<Vec<_>>();
        let other_campaigns_remaining = join_all(other_campaigns_remaining).await;
        let other_campaigns_remaining: Result<Vec<UnifiedNum>, _> = other_campaigns_remaining.into_iter().collect();
        other_campaigns_remaining
    }

    pub async fn get_campaigns_remaining_sum(redis: &MultiplexedConnection, campaigns: &[Campaign], mutated_campaign: &Campaign) -> Result<UnifiedNum, ResponseError> {
        let other_campaigns_remaining = get_remaining_for_multiple_campaigns(&redis, &campaigns, mutated_campaign.id).await?;
        let sum_of_campaigns_remaining = other_campaigns_remaining
            .into_iter()
            .try_fold(UnifiedNum::from_u64(0), |sum, val| sum.checked_add(&val).ok_or(ResponseError::Conflict("Couldn't sum remaining for campaigns".to_string())))?;

        // Necessary to do it explicitly for current campaign as its budget is not yet updated in DB
        let old_remaining_for_mutated_campaign = get_remaining_for_campaign_from_redis(&redis, mutated_campaign.id);
        let spent_for_mutated_campaign = mutated_campaign.budget.checked_sub(old_remaining_for_mutated_campaign);
        let new_remaining_for_mutated_campaign = mutated_campaign.budget.checked_sub(&spent_for_mutated_campaign).ok_or_else(|| {
            ResponseError::Conflict("Error while calculating remaining for mutated campaign".to_string())
        })?;
        sum_of_campaigns_remaining.checked_add(&new_remaining_for_mutated_campaign).ok_or_else(|| {
            ResponseError::Conflict("Error while calculating sum for all campaigns".to_string())
        })?;
        Ok(sum_of_campaigns_remaining)
    }
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
    async fn does_it_update_remaining() {
        let mut redis = get_redis().await;
        let campaign = get_campaign();
        let key = format!("remaining:{}", campaign.id);

        // Setting the redis base variable
        redis::cmd("SET")
            .arg(&key)
            .arg(100u64)
            .query_async::<_, ()>(&mut redis.connection)
            .await
            .expect("should set");

        // 2 async calls at once, should be 500 after them
        futures::future::join(
            update_remaining_for_campaign(&redis, campaign.id, UnifiedNum::from_u64(200)),
            update_remaining_for_campaign(&redis, campaign.id, UnifiedNum::from_u64(200))
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

        update_remaining_for_campaign(&redis, campaign.id, campaign.budget).await.expect("should increase");

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

        update_remaining_for_campaign(&redis, campaign.id, UnifiedNum::from_u64(0)).await.expect("should work");

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
        let key = format!("remaining:{}", campaign.id);

        let remaining = redis::cmd("GET")
            .arg(&key)
            .query_async::<_, Option<String>>(&mut redis.connection)
            .await;

        assert_eq!(remaining.is_err(), true)
    }

    // test get_campaigns_remaining_sum

    // test get_remaining_for_multiple_campaigns

    // test get_remaining_for_campaign_from_redis

    // test get_spent_for_campaign
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
