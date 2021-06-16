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
        SuccessResponse,
        accounting::Accounting,
    },
    spender::Spendable,
    validator::NewState,
    Address, Campaign, CampaignId, UnifiedNum, BigNum
};
use redis::aio::MultiplexedConnection;
use slog::error;
use crate::db::campaign::{update_campaign as update_campaign_db, insert_campaign, get_campaigns_for_channel};
use std::{
    cmp::max,
    str::FromStr,
};
use futures::future::join_all;

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
    let remaining_for_channel = get_total_remaining_for_channel(&campaign.creator, &accounting_spent, &latest_spendable)?;

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

    let update_response = SuccessResponse { success: true };

    Ok(success_response(serde_json::to_string(&campaign)?))
}

// TODO: Double check redis calls
async fn get_spent_for_campaign(redis: &MultiplexedConnection, id: CampaignId) -> Result<UnifiedNum, ResponseError> {
    let key = format!("spent:{}", id);
    // campaignSpent tracks the portion of the budget which has already been spent
    let campaign_spent = match redis::cmd("GET")
        .arg(&key)
        .query_async::<_, Option<String>>(&mut redis.clone())
        .await
        {
            Ok(Some(spent)) => {
                let res = BigNum::from_str(&spent)?;
                let res = res.to_u64().ok_or_else(|| {
                    ResponseError::Conflict("Error while converting BigNum to u64".to_string())
                })?;
                Ok(UnifiedNum::from_u64(res))
            },
            _ => Ok(UnifiedNum::from_u64(0))
        };

    campaign_spent
}

async fn get_remaining_for_campaign(redis: &MultiplexedConnection, id: CampaignId) -> Result<UnifiedNum, ResponseError> {
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

fn get_total_remaining_for_channel(creator: &Address, accounting_spent: &UnifiedNum, latest_spendable: &Spendable) -> Result<UnifiedNum, ResponseError> {
    let total_deposited = latest_spendable.deposit.total;

    let total_remaining = total_deposited.checked_sub(&accounting_spent).ok_or_else(|| {
        ResponseError::Conflict("Error while calculating the total remaining amount".to_string())
    })?;
    Ok(total_remaining)
}

// async fn update_remaining_for_channel(redis: &MultiplexedConnection, id: ChannelId, amount: UnifiedNum) -> Result<bool, PoolError> {
//     let key = format!("adexChannel:remaining:{}", id);
//     redis::cmd("SET")
//         .arg(&key)
//         .arg(amount.to_u64())
//         .query_async(&mut redis.clone())
//         .await?;
//     Ok(true)
// }

async fn get_campaigns_remaining_sum(redis: &MultiplexedConnection, pool: &DbPool, campaigns: &Vec<Campaign>, mutated_campaign: &Campaign) -> Result<UnifiedNum, ResponseError> {
    let other_campaigns_remaining = campaigns
        .into_iter()
        .filter(|c| c.id != mutated_campaign.id)
        .map(|c| async move {
            let spent = get_spent_for_campaign(&redis, c.id).await?;
            let remaining = c.budget.checked_sub(&spent)?;
            Ok(remaining)
        })
        .collect::<Vec<_>>();
    let other_campaigns_remaining = join_all(other_campaigns_remaining).await;
        // TODO: Fix Unwrap
    let sum_of_campaigns_remaining = other_campaigns_remaining
        .into_iter()
        .fold(UnifiedNum::from_u64(0), |mut sum, val| sum.checked_add(&val).unwrap());
    // Necessary to do it explicitly for current campaign as its budget is not yet updated in DB
    let spent_for_mutated_campaign = get_spent_for_campaign(&redis, mutated_campaign.id).await?;
    let remaining_for_mutated_campaign = mutated_campaign.budget.checked_sub(&spent_for_mutated_campaign).ok_or_else(|| {
        ResponseError::Conflict("Error while calculating remaining for mutated campaign".to_string())
    })?;
    sum_of_campaigns_remaining.checked_add(&remaining_for_mutated_campaign).ok_or_else(|| {
        ResponseError::Conflict("Error while calculating sum for all campaigns".to_string())
    });
    Ok(sum_of_campaigns_remaining)
}

pub async fn modify_campaign(pool: &DbPool, campaign: &Campaign, redis: &MultiplexedConnection) -> Result<bool, ResponseError> {
    let campaign_spent = get_spent_for_campaign(&redis, campaign.id).await?;
    let accounting_spent = get_accounting_spent(pool.clone(), &campaign.creator, &campaign.channel.id()).await?;

    let latest_spendable = fetch_spendable(pool.clone(), &campaign.creator, &campaign.channel.id()).await?;
    // Check if we have reached the budget
    if campaign_spent >= campaign.budget {
        return Err(ResponseError::FailedValidation("No more budget available for spending".into()));
    }

    let old_remaining = get_remaining_for_campaign(&redis, campaign.id).await?;
    let old_remaining = UnifiedNum::from_u64(max(0, old_remaining.to_u64()));

    let new_remaining = campaign.budget.checked_sub(&campaign_spent).ok_or_else(|| {
        ResponseError::Conflict("Error while subtracting campaign_spent from budget".to_string())
    })?;

    let diff_in_remaining = new_remaining.checked_sub(&old_remaining).ok_or_else(|| {
        ResponseError::Conflict("Error while subtracting campaign_spent from budget".to_string())
    })?;

    update_remaining_for_campaign(&redis, campaign.id, diff_in_remaining).await?;

    // Gets the latest Spendable for this (spender, channelId) pair
    let total_remaining = get_total_remaining_for_channel(&campaign.creator, &accounting_spent, &latest_spendable)?;
    let campaigns_for_channel = get_campaigns_for_channel(&pool, &campaign).await?;
    let campaigns_remaining_sum = get_campaigns_remaining_sum(&redis, &pool, &campaigns_for_channel, &campaign).await?;
    if campaigns_remaining_sum > total_remaining {
        return Err(ResponseError::Conflict("Remaining for campaigns exceeds total remaining for channel".to_string()));
    }

    update_campaign_db(&pool, &campaign).await.map_err(|e| ResponseError::Conflict(e.to_string()))

    // *NOTE*: When updating campaigns make sure sum(campaigns.map(getRemaining)) <= totalDepoisted - totalspent
    // !WARNING!: totalSpent != sum(campaign.map(c => c.spending)) therefore we must always calculate remaining funds based on total_deposit - lastApprovedNewState.spenders[user]
    // *NOTE*: To close a campaign set campaignBudget to campaignSpent so that spendable == 0
}



#[cfg(test)]
mod test {
    use primitives::{
        util::tests::prep_db::{DUMMY_CAMPAIGN, DUMMY_CHANNEL},
        Deposit
    };

    use deadpool::managed::Object;

    use crate::{
        db::redis_pool::{Manager, TESTS_POOL},
    };

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
            channel: DUMMY_CHANNEL.clone(),
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

        let total_remaining = get_total_remaining_for_channel(&campaign.creator, &accounting_spent, &latest_spendable);

        assert_eq!(total_remaining, UnifiedNum::from_u64(900_000));
    }

    #[tokio::test]
    async fn does_it_update_remaining() {
        let redis = get_redis().await;
        let campaign = get_campaign();
        let key = format!("remaining:{}", campaign.id);

        // Setting the redis base variable
        redis::cmd("SET")
            .arg(&key)
            .arg(100u64)
            .query_async(&mut redis.connection)
            .await
            .expect("should set");

        // 2 async calls at once, should be 500 after them
        futures::future::join(
            update_remaining_for_campaign(&redis, campaign.id, UnifiedNum::from_u64(200)),
            update_remaining_for_campaign(&redis, campaign.id, UnifiedNum::from_u64(200))
        ).await;

        let remaining = redis::cmd("GET")
            .arg(&key)
            .query_async::<_, Option<String>>(&mut redis.clone())
            .await
            .expect("should get remaining");

        assert_eq!(remaining, UnifiedNum::from_u64(500));

        update_remaining_for_campaign(&redis, campaign.id, campaign.budget).await.expect("should increase");

        let remaining = redis::cmd("GET")
            .arg(&key)
            .query_async::<_, Option<String>>(&mut redis.clone())
            .await
            .expect("should get remaining");

        let should_be_remaining = UnifiedNum::from_u64(500).checked_add(&campaign.budget).expect("should add");
        assert_eq!(remaining, should_be_remaining);

        update_remaining_for_campaign(&redis, campaign.id, UnifiedNum::from_u64(0)).await.expect("should work");

        let remaining = redis::cmd("GET")
            .arg(&key)
            .query_async::<_, Option<String>>(&mut redis.clone())
            .await
            .expect("should get remaining");

        assert_eq!(remaining, should_be_remaining);

        update_remaining_for_campaign(&redis, campaign.id, UnifiedNum::from_u64(-500)).await.expect("should work");

        let should_be_remaining = should_be_remaining.checked_sub(500).expect("should work");

        let remaining = redis::cmd("GET")
            .arg(&key)
            .query_async::<_, Option<String>>(&mut redis.clone())
            .await
            .expect("should get remaining");

        assert_eq!(remaining, should_be_remaining);
    }

    #[tokio::test]
    async fn update_remaining_before_it_is_set() {
        let redis = get_redis().await;
        let campaign = get_campaign();
        let key = format!("remaining:{}", campaign.id);

        let remaining = redis::cmd("GET")
            .arg(&key)
            .query_async::<_, Option<String>>(&mut redis.clone())
            .await;

        assert_eq!(remaining, Err(ResponseError::Conflict))
    }
}