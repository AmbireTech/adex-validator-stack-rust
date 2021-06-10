use crate::{
    success_response, Application, ResponseError,
    db::{
        spendable::fetch_spendable,
        event_aggregate::latest_new_state_v5,
        DbPool
    },
};
use hyper::{Body, Request, Response};
use primitives::{
    adapter::Adapter,
    sentry::{
        campaign_create::CreateCampaign,
        SuccessResponse,
        MessageResponse
    },
    spender::Spendable,
    validator::NewState,
    Campaign, CampaignId, UnifiedNum, BigNum
};
use redis::aio::MultiplexedConnection;
use slog::error;
use crate::db::campaign::{update_campaign_in_db, insert_campaign, get_campaigns_for_channel};
use std::str::FromStr;

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

    // Checking if there is enough remaining deposit 
    // TODO: Switch with Accounting once it's ready
    let latest_new_state = latest_new_state_v5(&app.pool, &campaign.channel, "").await?;

    // TODO: AIP#61: Update when changes to Spendable are ready
    let latest_spendable = fetch_spendable(app.pool.clone(), &campaign.creator, &campaign.channel.id()).await?;
    let remaining_for_channel = get_total_remaining_for_channel(&app.redis, &app.pool, &campaign, &latest_new_state, &latest_spendable).await?;

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

    let create_response = SuccessResponse { success: true };

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
    let key = format!("adexCampaign:campaignSpent:{}", id);
    // campaignSpent tracks the portion of the budget which has already been spent
    let campaign_spent = match redis::cmd("GET")
        .arg(&key)
        .query_async::<_, Option<String>>(&mut redis.clone())
        .await
        .map_err(|_| ResponseError::Conflict("Error getting campaignSpent for current campaign".to_string()))?{
            Some(spent) => {
                // TODO: Fix unwraps
                let res = BigNum::from_str(&spent).unwrap();
                let res = res.to_u64().unwrap();
                UnifiedNum::from_u64(res)
            },
            None => UnifiedNum::from_u64(0)
        };

    Ok(campaign_spent)
}

async fn update_remaining_for_campaign(redis: &MultiplexedConnection, id: CampaignId, amount: UnifiedNum) -> Result<bool, ResponseError> {
    // update a key in Redis for the remaining spendable amount
    let key = format!("adexCampaign:remainingSpendable:{}", id);
    redis::cmd("SET")
        .arg(&key)
        .arg(amount.to_u64())
        .query_async(&mut redis.clone())
        .await
        .map_err(|_| ResponseError::Conflict("Error updating remainingSpendable for current campaign".to_string()))?
}

async fn get_total_remaining_for_channel(redis: &MultiplexedConnection, pool: &DbPool, campaign: &Campaign, latest_new_state: &Option<MessageResponse<NewState>>, latest_spendable: &Spendable) -> Result<UnifiedNum, ResponseError> {
    let total_deposited = latest_spendable.deposit.total;

    let latest_new_state = latest_new_state.as_ref().ok_or_else(|| ResponseError::Conflict("Error getting latest new state message".to_string()))?;
    let msg = &latest_new_state.msg;
    let total_spent = msg.balances.get(&campaign.creator);
    let zero = BigNum::from(0);
    let total_spent = if let Some(spent) = total_spent {
        spent
    } else {
        &zero
    };

    // TODO: total_spent is BigNum, is it safe to just convert it to UnifiedNum like this?
    let total_spent = total_spent.to_u64().ok_or_else(|| {
        ResponseError::Conflict("Error while converting total_spent to u64".to_string())
    })?;
    let total_remaining = total_deposited.checked_sub(&UnifiedNum::from_u64(total_spent)).ok_or_else(|| {
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
    let other_campaigns_remaining: Vec<UnifiedNum> = campaigns
        .into_iter()
        .filter(|c| c.id != mutated_campaign.id)
        .map(|c| async move {
            let spent = get_spent_for_campaign(&redis, c.id).await?;
            let remaining = c.budget.checked_sub(&spent)?;
            Ok(remaining)
        })
        .collect();
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
    // Getting the latest new state from Postgres
    let latest_new_state = latest_new_state_v5(&pool, &campaign.channel, "").await?;

    let latest_spendable = fetch_spendable(pool.clone(), &campaign.creator, &campaign.channel.id()).await?;
    // Check if we have reached the budget
    if campaign_spent >= campaign.budget {
        return Err(ResponseError::FailedValidation("No more budget available for spending".into()));
    }

    let remaining_spendable_campaign = campaign.budget.checked_sub(&campaign_spent).ok_or_else(|| {
        ResponseError::Conflict("Error while subtracting campaign_spent from budget".to_string())
    })?;
    update_remaining_for_campaign(&redis, campaign.id, remaining_spendable_campaign).await?;

    // Gets the latest Spendable for this (spender, channelId) pair
    let total_remaining = get_total_remaining_for_channel(&redis, &pool, &campaign, &latest_new_state, &latest_spendable).await?;
    let campaigns_for_channel = get_campaigns_for_channel(&pool, &campaign).await?;
    let campaigns_remaining_sum = get_campaigns_remaining_sum(&redis, &pool, &campaigns_for_channel, &campaign).await?;
    if campaigns_remaining_sum > total_remaining {
        return Err(ResponseError::Conflict("Remaining for campaigns exceeds total remaining for channel".to_string()));
    }

    update_campaign_in_db(&pool, &campaign).await.map_err(|e| ResponseError::Conflict(e.to_string()))

    // *NOTE*: When updating campaigns make sure sum(campaigns.map(getRemaining)) <= totalDepoisted - totalspent
    // !WARNING!: totalSpent != sum(campaign.map(c => c.spending)) therefore we must always calculate remaining funds based on total_deposit - lastApprovedNewState.spenders[user]
    // *NOTE*: To close a campaign set campaignBudget to campaignSpent so that spendable == 0
}
