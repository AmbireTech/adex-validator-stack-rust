//! `/v5/campaign` routes
use crate::{
    db::{
        accounting::{get_accounting, Side},
        campaign::{
            get_campaign_ids_by_channel, list_campaigns, list_campaigns_total_count,
            update_campaign,
        },
        insert_campaign, insert_channel,
        spendable::update_spendable,
        CampaignRemaining, DbPool, RedisError,
    },
    response::{success_response, ResponseError},
    Application, Auth,
};
use adapter::{prelude::*, Adapter, Error as AdaptorError};
use deadpool_postgres::PoolError;
use futures::{future::try_join_all, TryFutureExt};
use hyper::{Body, Request, Response};
use primitives::{
    campaign_validator::Validator,
    sentry::{
        campaign_create::CreateCampaign, campaign_list::CampaignListQuery,
        campaign_modify::ModifyCampaign, SuccessResponse,
    },
    spender::Spendable,
    Address, Campaign, CampaignId, ChainOf, Channel, ChannelId, Deposit, UnifiedNum,
};
use slog::error;
use std::cmp::{max, Ordering};
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
    #[error("Channel token is not whitelisted")]
    ChannelTokenNotWhitelisted,
    #[error("Campaign was not modified because of spending constraints")]
    CampaignNotModified,
    #[error("Error while updating spendable for creator: {0}")]
    LatestSpendable(#[from] LatestSpendableError),
    #[error("Redis error: {0}")]
    Redis(#[from] RedisError),
    #[error("DB Pool error: {0}")]
    Pool(#[from] PoolError),
}

#[derive(Debug, Error)]
pub enum LatestSpendableError {
    #[error("Adapter: {0}")]
    Adapter(#[from] AdaptorError),
    #[error("Overflow occurred while converting native token precision to unified precision")]
    Overflow,
    #[error("DB Pool error: {0}")]
    Pool(#[from] PoolError),
}
/// Gets the latest Spendable from the Adapter and updates it in the Database
/// before returning it
pub async fn update_latest_spendable<C>(
    adapter: &Adapter<C>,
    pool: &DbPool,
    channel_context: &ChainOf<Channel>,
    address: Address,
) -> Result<Spendable, LatestSpendableError>
where
    C: Locked + 'static,
{
    let latest_deposit = adapter.get_deposit(channel_context, address).await?;

    let spendable = Spendable {
        spender: address,
        channel: channel_context.context,
        deposit: Deposit::<UnifiedNum>::from_precision(
            latest_deposit,
            channel_context.token.precision.get(),
        )
        .ok_or(LatestSpendableError::Overflow)?,
    };

    Ok(update_spendable(pool.clone(), &spendable).await?)
}

pub async fn fetch_campaign_ids_for_channel(
    pool: &DbPool,
    channel_id: ChannelId,
    limit: u32,
) -> Result<Vec<CampaignId>, ResponseError> {
    let campaign_ids = get_campaign_ids_by_channel(pool, &channel_id, limit.into(), 0).await?;

    let total_count = list_campaigns_total_count(
        pool,
        (
            &["campaigns.channel_id = $1".to_string()],
            vec![&channel_id],
        ),
    )
    .await?;

    // fast ceil for total_pages
    let total_pages = if total_count == 0 {
        1
    } else {
        1 + ((total_count - 1) / limit as u64)
    };

    if total_pages < 2 {
        Ok(campaign_ids)
    } else {
        let pages_skip: Vec<u64> = (1..total_pages)
            .map(|i| {
                i.checked_mul(limit.into()).ok_or_else(|| {
                    ResponseError::FailedValidation(
                        "Calculating skip while fetching campaign ids results in an overflow"
                            .to_string(),
                    )
                })
            })
            .collect::<Result<_, _>>()?;

        let other_pages = try_join_all(pages_skip.into_iter().map(|skip| {
            get_campaign_ids_by_channel(pool, &channel_id, limit.into(), skip)
                .map_err(|e| ResponseError::BadRequest(e.to_string()))
        }))
        .await?;

        let all_campaigns = std::iter::once(campaign_ids)
            .chain(other_pages.into_iter())
            .flat_map(|campaign_ids| campaign_ids.into_iter())
            .collect();

        Ok(all_campaigns)
    }
}

/// POST `/v5/campaign`
pub async fn create_campaign<C>(
    req: Request<Body>,
    app: &Application<C>,
) -> Result<Response<Body>, ResponseError>
where
    C: Locked + 'static,
{
    let auth = req
        .extensions()
        .get::<Auth>()
        .expect("request should have session")
        .to_owned();

    let body = hyper::body::to_bytes(req.into_body()).await?;

    let campaign_context = serde_json::from_slice::<CreateCampaign>(&body)
        .map_err(|e| ResponseError::FailedValidation(e.to_string()))?
        // create the actual `Campaign` with a randomly generated `CampaignId` or the set `CampaignId`
        .into_campaign()
        // Validate the campaign as soon as a valid JSON was passed.
        // This will validate the Context - Chain & Token are whitelisted!
        .validate(&app.config, app.adapter.whoami())
        .map_err(|err| ResponseError::FailedValidation(err.to_string()))?;
    let campaign = &campaign_context.context;

    if auth.uid.to_address() != campaign.creator {
        return Err(ResponseError::Forbidden(
            "Request not sent by campaign creator".to_string(),
        ));
    }

    let channel_context = app
        .config
        .find_chain_of(campaign.channel.token)
        .ok_or_else(|| {
            ResponseError::FailedValidation(
                "Channel token is not whitelisted in this validator".into(),
            )
        })?
        .with_channel(campaign.channel);
    // make sure that the Channel is available in the DB
    // insert Channel
    insert_channel(&app.pool, &channel_context)
        .await
        .map_err(|error| {
            error!(&app.logger, "{}", &error; "module" => "create_campaign");

            ResponseError::BadRequest("Failed to fetch/create Channel".to_string())
        })?;

    let total_remaining = {
        let accounting_spent = get_accounting(
            app.pool.clone(),
            campaign.channel.id(),
            campaign.creator,
            Side::Spender,
        )
        .await?
        .map(|accounting| accounting.amount)
        .unwrap_or_default();

        let latest_spendable = update_latest_spendable(
            &app.adapter,
            &app.pool,
            &campaign_context.of_channel(),
            campaign.creator,
        )
        .await
        .map_err(|err| ResponseError::BadRequest(err.to_string()))?;
        // Gets the latest Spendable for this (spender, channelId) pair
        let total_deposited = latest_spendable.deposit.total;

        total_deposited
            .checked_sub(&accounting_spent)
            .ok_or_else(|| {
                ResponseError::FailedValidation("No more budget remaining".to_string())
            })?
    };

    let channel_campaigns = fetch_campaign_ids_for_channel(
        &app.pool,
        campaign.channel.id(),
        app.config.campaigns_find_limit,
    )
    .await?;

    let campaigns_remaining_sum = app
        .campaign_remaining
        .get_multiple(channel_campaigns.as_slice())
        .await?
        .iter()
        .sum::<Option<UnifiedNum>>()
        .ok_or(Error::Calculation)?
        // DO NOT FORGET to add the Campaign being created right now!
        .checked_add(&campaign.budget)
        .ok_or(Error::Calculation)?;

    // `new_campaigns_remaining <= total_remaining` should be upheld
    // `campaign.budget < total_remaining` should also be upheld!
    if campaigns_remaining_sum > total_remaining || campaign.budget > total_remaining {
        return Err(ResponseError::BadRequest(
            "Not enough deposit left for the new campaign's budget".to_string(),
        ));
    }

    // If the campaign is being created, the amount spent is 0, therefore remaining = budget
    let remaining_set = CampaignRemaining::new(app.redis.clone())
        .set_initial(campaign.id, campaign.budget)
        .await
        .map_err(|_| {
            ResponseError::BadRequest("Couldn't set remaining while creating campaign".to_string())
        })?;

    // If for some reason the randomly generated `CampaignId` exists in Redis
    // This should **NOT** happen!
    if !remaining_set {
        return Err(ResponseError::Conflict(
            "The generated CampaignId already exists, please repeat the request".to_string(),
        ));
    }

    // Channel insertion can never create a `SqlState::UNIQUE_VIOLATION`
    // Insert the Campaign too
    match insert_campaign(&app.pool, campaign).await {
        Err(error) => {
            error!(&app.logger, "{}", &error; "module" => "create_campaign");
            match error {
                PoolError::Backend(error) if error.code() == Some(&SqlState::UNIQUE_VIOLATION) => {
                    Err(ResponseError::Conflict(
                        "Campaign already exists".to_string(),
                    ))
                }
                _err => Err(ResponseError::BadRequest(
                    "Error occurred when inserting Campaign in Database; please try again later"
                        .to_string(),
                )),
            }
        }
        Ok(false) => Err(ResponseError::BadRequest(
            "Encountered error while creating Campaign; please try again".to_string(),
        )),
        _ => Ok(()),
    }?;

    Ok(success_response(serde_json::to_string(&campaign)?))
}

/// GET `/v5/campaign/list`
pub async fn campaign_list<C: Locked + 'static>(
    req: Request<Body>,
    app: &Application<C>,
) -> Result<Response<Body>, ResponseError> {
    let query = serde_qs::from_str::<CampaignListQuery>(req.uri().query().unwrap_or(""))?;

    let limit = app.config.campaigns_find_limit;
    let skip = query
        .page
        .checked_mul(limit.into())
        .ok_or_else(|| ResponseError::BadRequest("Page and/or limit is too large".into()))?;
    let list_response = list_campaigns(
        &app.pool,
        skip,
        limit,
        query.creator,
        query.validator,
        &query.active_to_ge,
    )
    .await?;

    Ok(success_response(serde_json::to_string(&list_response)?))
}

/// POST `/v5/campaign/:id/close` (auth required)
///
/// Can only be called by the [`Campaign.creator`]!
/// To close a campaign, just set it's budget to what it's spent so far (so that remaining == 0)
/// newBudget = totalSpent, i.e. newBudget = oldBudget - remaining
pub async fn close_campaign<C: Locked + 'static>(
    req: Request<Body>,
    app: &Application<C>,
) -> Result<Response<Body>, ResponseError> {
    let auth = req
        .extensions()
        .get::<Auth>()
        .expect("Auth should be present");

    let campaign_context = req
        .extensions()
        .get::<ChainOf<Campaign>>()
        .expect("We must have a campaign in extensions")
        .to_owned();
    let mut campaign = campaign_context.context;

    if auth.uid.to_address() != campaign.creator {
        Err(ResponseError::Forbidden(
            "Request not sent by campaign creator".to_string(),
        ))
    } else {
        let old_remaining = app
            .campaign_remaining
            .getset_remaining_to_zero(campaign.id)
            .await
            .map_err(|e| ResponseError::BadRequest(e.to_string()))?;

        campaign.budget = campaign
            .budget
            .checked_sub(&UnifiedNum::from(old_remaining))
            .ok_or_else(|| {
                ResponseError::BadRequest("Campaign budget overflow/underflow".to_string())
            })?;
        update_campaign(&app.pool, &campaign).await?;

        Ok(success_response(serde_json::to_string(&SuccessResponse {
            success: true,
        })?))
    }
}

pub mod update_campaign {
    use primitives::Config;

    use crate::db::{accounting::Side, CampaignRemaining};

    use super::*;

    /// POST `/v5/campaign/:id` (auth required)
    pub async fn handle_route<C: Locked + 'static>(
        req: Request<Body>,
        app: &Application<C>,
    ) -> Result<Response<Body>, ResponseError> {
        let campaign_being_mutated = req
            .extensions()
            .get::<ChainOf<Campaign>>()
            .expect("We must have a campaign in extensions")
            .clone();

        let body = hyper::body::to_bytes(req.into_body()).await?;

        let modify_campaign_fields = serde_json::from_slice::<ModifyCampaign>(&body)
            .map_err(|e| ResponseError::FailedValidation(e.to_string()))?;

        // modify Campaign
        let modified_campaign = modify_campaign(
            app.adapter.clone(),
            &app.pool,
            &app.config,
            &app.campaign_remaining,
            &campaign_being_mutated,
            modify_campaign_fields,
        )
        .await
        .map_err(|err| ResponseError::BadRequest(err.to_string()))?;

        Ok(success_response(serde_json::to_string(&modified_campaign)?))
    }

    pub async fn modify_campaign<C: Locked + 'static>(
        adapter: Adapter<C>,
        pool: &DbPool,
        config: &Config,
        campaign_remaining: &CampaignRemaining,
        campaign_context: &ChainOf<Campaign>,
        modify_campaign: ModifyCampaign,
    ) -> Result<Campaign, Error> {
        let campaign = &campaign_context.context;
        // *NOTE*: When updating campaigns make sure sum(campaigns.map(getRemaining)) <= totalDeposited - totalSpent
        // !WARNING!: totalSpent != sum(campaign.map(c => c.spending)) therefore we must always calculate remaining funds based on total_deposit - lastApprovedNewState.spenders[user]
        // *NOTE*: To close a campaign set campaignBudget to campaignSpent so that spendable == 0

        let delta_budget = if let Some(new_budget) = modify_campaign.budget {
            get_delta_budget(campaign_remaining, campaign, new_budget).await?
        } else {
            None
        };

        // if we are going to update the budget
        // validate the totalDeposit - totalSpent for all campaign
        // sum(AllChannelCampaigns.map(getRemaining)) + DeltaBudgetForMutatedCampaign <= totalDeposited - totalSpent
        // sum(AllChannelCampaigns.map(getRemaining)) - DeltaBudgetForMutatedCampaign <= totalDeposited - totalSpent
        if let Some(delta_budget) = delta_budget {
            let accounting_spent = get_accounting(
                pool.clone(),
                campaign.channel.id(),
                campaign.creator,
                Side::Spender,
            )
            .await?
            .map(|accounting| accounting.amount)
            .unwrap_or_default();

            let latest_spendable = update_latest_spendable(
                &adapter,
                pool,
                &campaign_context.of_channel(),
                campaign.creator,
            )
            .await?;

            // Gets the latest Spendable for this (spender, channelId) pair
            let total_deposited = latest_spendable.deposit.total;

            let total_remaining = total_deposited
                .checked_sub(&accounting_spent)
                .ok_or(Error::Calculation)?;

            let channel_campaigns = fetch_campaign_ids_for_channel(
                pool,
                campaign.channel.id(),
                config.campaigns_find_limit,
            )
            .await
            .map_err(|_| Error::FailedUpdate("couldn't fetch campaigns for channel".to_string()))?;

            // this will include the Campaign we are currently modifying
            let campaigns_current_remaining_sum = campaign_remaining
                .get_multiple(channel_campaigns.as_slice())
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

            // `new_campaigns_remaining <= total_remaining` should be upheld
            if new_campaigns_remaining > total_remaining {
                return Err(Error::NewBudget(
                    "Not enough deposit left for the campaign's new budget".to_string(),
                ));
            }

            // there is a chance that the new remaining will be negative even when increasing the budget
            // We don't currently use this value but can be used to perform additional checks or return messages accordingly
            let _campaign_remaining = match delta_budget {
                DeltaBudget::Increase(increase_by) => {
                    campaign_remaining
                        .increase_by(campaign.id, increase_by)
                        .await?
                }
                DeltaBudget::Decrease(decrease_by) => {
                    campaign_remaining
                        .decrease_by(campaign.id, decrease_by)
                        .await?
                }
            };
        }

        let modified_campaign = modify_campaign.apply(campaign.clone());
        update_campaign(pool, &modified_campaign).await?;

        Ok(modified_campaign)
    }

    /// Delta Budget describes the difference between the New and Old budget
    /// It is used to decrease or increase the remaining budget instead of setting it up directly
    /// This way if a new event alters the remaining budget in Redis while the modification of campaign hasn't finished
    /// it will correctly update the remaining using an atomic redis operation with `INCRBY` or `DECRBY` instead of using `SET`
    #[derive(Debug, Clone, PartialEq)]
    pub(super) enum DeltaBudget<T> {
        Increase(T),
        Decrease(T),
    }

    // TODO: Figure out a way to simplify Errors and remove the Adapter from here
    pub(super) async fn get_delta_budget(
        campaign_remaining: &CampaignRemaining,
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

        let old_remaining = campaign_remaining
            .get_remaining_opt(campaign.id)
            .await?
            .map(|remaining| UnifiedNum::from(max(0, remaining).unsigned_abs()))
            .ok_or_else(|| Error::FailedUpdate("No remaining entry for campaign".to_string()))?;

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
                // new remaining > old remaining
                let increase_by = new_remaining
                    .checked_sub(&old_remaining)
                    .ok_or(Error::Calculation)?;

                DeltaBudget::Increase(increase_by)
            }
            DeltaBudget::Decrease(()) => {
                // delta budget = Old budget - New budget ( the difference between the new and old when New < Old)
                let new_remaining = &current_budget
                    .checked_sub(&new_budget)
                    .and_then(|delta_budget| old_remaining.checked_sub(&delta_budget))
                    .ok_or(Error::Calculation)?;
                // old remaining > new remaining
                let decrease_by = old_remaining
                    .checked_sub(new_remaining)
                    .ok_or(Error::Calculation)?;

                DeltaBudget::Decrease(decrease_by)
            }
        };

        Ok(Some(budget))
    }
}

pub mod insert_events {

    use std::collections::HashMap;

    use crate::{
        access::{self, check_access},
        analytics,
        db::{accounting::spend_amount, CampaignRemaining, DbPool, PoolError, RedisError},
        payout::get_payout,
        response::ResponseError,
        spender::fee::calculate_fee,
        Application, Auth, Session,
    };
    use adapter::prelude::*;
    use hyper::{Body, Request, Response};
    use primitives::{
        balances::{Balances, CheckedState, OverflowError},
        sentry::{Event, SuccessResponse},
        Address, Campaign, CampaignId, ChainOf, DomainError, UnifiedNum, ValidatorDesc,
    };
    use slog::error;
    use thiserror::Error;

    #[derive(Debug, Error)]
    pub enum Error {
        #[error(transparent)]
        Event(#[from] EventError),
        #[error(transparent)]
        Redis(#[from] RedisError),
        #[error(transparent)]
        Postgres(#[from] PoolError),
        #[error(transparent)]
        Overflow(#[from] OverflowError),
    }

    #[derive(Debug, Error, PartialEq)]
    pub enum EventError {
        #[error("Overflow when calculating Event payout for Event")]
        EventPayoutOverflow,
        #[error("Validator Fee calculation: {0}")]
        FeeCalculation(#[from] DomainError),
        #[error(
            "The Campaign's remaining budget left to spend is not enough to cover the Event payout"
        )]
        CampaignRemainingNotEnoughForPayout,
        #[error("Campaign ran out of remaining budget to spend")]
        CampaignOutOfBudget,
    }

    /// POST `/v5/campaign/:id`
    pub async fn handle_route<C: Locked + 'static>(
        req: Request<Body>,
        app: &Application<C>,
    ) -> Result<Response<Body>, ResponseError> {
        let (req_head, req_body) = req.into_parts();

        let auth = req_head.extensions.get::<Auth>();
        let session = req_head
            .extensions
            .get::<Session>()
            .expect("request should have session");

        let campaign_context = req_head
            .extensions
            .get::<ChainOf<Campaign>>()
            .expect("request should have a Campaign loaded");

        let body_bytes = hyper::body::to_bytes(req_body).await?;
        let mut request_body = serde_json::from_slice::<HashMap<String, Vec<Event>>>(&body_bytes)?;

        let events = request_body
            .remove("events")
            .ok_or_else(|| ResponseError::BadRequest("invalid request".to_string()))?;

        let processed = process_events(app, auth, session, campaign_context, events).await?;

        Ok(Response::builder()
            .header("Content-type", "application/json")
            .body(serde_json::to_string(&SuccessResponse { success: processed })?.into())
            .unwrap())
    }

    async fn process_events<C: Locked + 'static>(
        app: &Application<C>,
        auth: Option<&Auth>,
        session: &Session,
        campaign_context: &ChainOf<Campaign>,
        events: Vec<Event>,
    ) -> Result<bool, ResponseError> {
        let campaign = &campaign_context.context;

        // handle events - check access
        check_access(
            &app.redis,
            session,
            auth,
            &app.config.ip_rate_limit,
            &campaign_context.context,
            &events,
        )
        .await
        .map_err(|e| match e {
            access::Error::ForbiddenReferrer => ResponseError::Forbidden(e.to_string()),
            access::Error::RulesError(error) => ResponseError::TooManyRequests(error),
            access::Error::UnAuthenticated => ResponseError::Unauthorized,
            _ => ResponseError::BadRequest(e.to_string()),
        })?;

        let (leader, follower) = match (campaign.leader(), campaign.follower()) {
            // ERROR!
            (None, None) | (None, _) | (_, None) => {
                return Err(ResponseError::BadRequest(
                    "Channel leader, follower or both were not found in Campaign validators."
                        .to_string(),
                ))
            }
            (Some(leader), Some(follower)) => (leader, follower),
        };

        let mut events_success = vec![];
        for event in events.into_iter() {
            let event_payout = {
                // calculate earners payouts
                let payout = get_payout(&app.logger, &campaign_context.context, &event, session)?;

                match payout {
                    Some((earner, payout)) => {
                        let spending_result = spend_for_event(
                            &app.pool,
                            &app.campaign_remaining,
                            &campaign_context.context,
                            earner,
                            leader,
                            follower,
                            payout,
                        )
                        .await;

                        // Log unsuccessfully spending
                        match spending_result {
                            Ok(()) => Some((event, earner, payout)),
                            Err(err) => {
                                error!(&app.logger, "Payout spending failed: {}", err; "campaign" => ?campaign_context, "event" => ?event, "earner" => ?earner, "unpaid amount" => %payout, "err" => ?err);

                                None
                            }
                        }
                    }
                    // if None, then ad was never show
                    None => None,
                }
            };

            if let Some(event_payout) = event_payout {
                events_success.push(event_payout);
            }
        }

        // Record successfully paid out events to Analytics
        if let Err(err) =
            analytics::record(&app.pool, campaign_context, session, events_success).await
        {
            error!(&app.logger, "Analytics recording failed: {}", err; "campaign" => ?campaign_context, "err" => ?err)
        }

        Ok(true)
    }

    /// This function calculates the fee for each validator.
    ///
    /// It then spends the given amounts for:
    ///
    /// - `Publisher` - payout
    ///
    /// `Leader` and `Follower` - Validator fees
    pub async fn spend_for_event(
        pool: &DbPool,
        campaign_remaining: &CampaignRemaining,
        campaign: &Campaign,
        earner: Address,
        leader: &ValidatorDesc,
        follower: &ValidatorDesc,
        amount: UnifiedNum,
    ) -> Result<(), Error> {
        // distribute fees
        let leader_fee =
            calculate_fee((earner, amount), leader).map_err(EventError::FeeCalculation)?;
        let follower_fee =
            calculate_fee((earner, amount), follower).map_err(EventError::FeeCalculation)?;

        // First update redis `campaignRemaining:{CampaignId}` key
        let spending = [amount, leader_fee, follower_fee]
            .iter()
            .sum::<Option<UnifiedNum>>()
            .ok_or(EventError::EventPayoutOverflow)?;

        if !has_enough_remaining_budget(campaign_remaining, campaign.id, spending).await? {
            return Err(Error::Event(
                EventError::CampaignRemainingNotEnoughForPayout,
            ));
        }

        // The event payout decreases the remaining budget for the Campaign
        let remaining = campaign_remaining
            .decrease_by(campaign.id, spending)
            .await?;

        // Update the Accounting records accordingly
        let channel_id = campaign.channel.id();
        let spender = campaign.creator;

        let mut delta_balances = Balances::<CheckedState>::default();
        delta_balances.spend(spender, earner, amount)?;
        delta_balances.spend(spender, leader.id.to_address(), leader_fee)?;
        delta_balances.spend(spender, follower.id.to_address(), follower_fee)?;

        let (_earners, _spenders) = spend_amount(pool.clone(), channel_id, delta_balances).await?;

        // check if we still have budget to spend, after we've updated both Redis and Postgres
        if remaining.is_negative() {
            Err(Error::Event(EventError::CampaignOutOfBudget))
        } else {
            Ok(())
        }
    }

    async fn has_enough_remaining_budget(
        campaign_remaining: &CampaignRemaining,
        campaign: CampaignId,
        amount: UnifiedNum,
    ) -> Result<bool, RedisError> {
        let remaining = campaign_remaining
            .get_remaining_opt(campaign)
            .await?
            .unwrap_or_default();

        Ok(remaining > 0 && remaining.unsigned_abs() > amount.to_u64())
    }

    #[cfg(test)]
    mod test {
        use primitives::{
            test_util::{DUMMY_CAMPAIGN, PUBLISHER},
            unified_num::FromWhole,
        };
        use redis::aio::MultiplexedConnection;

        use crate::{
            db::{
                insert_channel,
                redis_pool::TESTS_POOL,
                tests_postgres::{setup_test_migrations, DATABASE_POOL},
            },
            test_util::setup_dummy_app,
        };

        use super::*;

        /// Helper function to set the Campaign Remaining budget in Redis for the tests
        async fn set_campaign_remaining(
            redis: &mut MultiplexedConnection,
            campaign: CampaignId,
            remaining: i64,
        ) {
            let key = CampaignRemaining::get_key(campaign);

            redis::cmd("SET")
                .arg(&key)
                .arg(remaining)
                .query_async::<_, ()>(redis)
                .await
                .expect("Should set Campaign remaining key");
        }

        #[tokio::test]
        async fn test_has_enough_remaining_budget() {
            let mut redis = TESTS_POOL.get().await.expect("Should get redis connection");
            let campaign_remaining = CampaignRemaining::new(redis.connection.clone());
            let campaign = DUMMY_CAMPAIGN.id;
            let amount = UnifiedNum::from(10_000);

            let no_remaining_budget_set =
                has_enough_remaining_budget(&campaign_remaining, campaign, amount)
                    .await
                    .expect("Should check campaign remaining");
            assert!(
                !no_remaining_budget_set,
                "No remaining budget set, should return false"
            );

            set_campaign_remaining(&mut redis, campaign, 9_000).await;

            let not_enough_remaining_budget =
                has_enough_remaining_budget(&campaign_remaining, campaign, amount)
                    .await
                    .expect("Should check campaign remaining");
            assert!(
                !not_enough_remaining_budget,
                "Not enough remaining budget, should return false"
            );

            set_campaign_remaining(&mut redis, campaign, 11_000).await;

            let has_enough_remaining_budget =
                has_enough_remaining_budget(&campaign_remaining, campaign, amount)
                    .await
                    .expect("Should check campaign remaining");

            assert!(
                has_enough_remaining_budget,
                "Should have enough budget for this amount"
            );
        }

        #[tokio::test]
        async fn test_decreasing_remaining_budget() {
            let mut redis = TESTS_POOL.get().await.expect("Should get redis connection");
            let campaign = DUMMY_CAMPAIGN.id;
            let campaign_remaining = CampaignRemaining::new(redis.connection.clone());
            let amount = UnifiedNum::from(5_000);

            set_campaign_remaining(&mut redis, campaign, 9_000).await;

            let remaining = campaign_remaining
                .decrease_by(campaign, amount)
                .await
                .expect("Should decrease campaign remaining");
            assert_eq!(
                4_000, remaining,
                "Should decrease remaining budget with amount and be positive"
            );

            let remaining = campaign_remaining
                .decrease_by(campaign, amount)
                .await
                .expect("Should decrease campaign remaining");
            assert_eq!(
                -1_000, remaining,
                "Should decrease remaining budget with amount and be negative"
            );
        }

        #[tokio::test]
        async fn test_spending_for_events_with_enough_remaining_budget() {
            let mut redis = TESTS_POOL.get().await.expect("Should get redis connection");
            let database = DATABASE_POOL.get().await.expect("Should get a DB pool");
            let campaign_remaining = CampaignRemaining::new(redis.connection.clone());
            let app = setup_dummy_app().await;

            setup_test_migrations(database.pool.clone())
                .await
                .expect("Migrations should succeed");

            let campaign = DUMMY_CAMPAIGN.clone();
            let channel_chain = app
                .config
                .find_chain_of(DUMMY_CAMPAIGN.channel.token)
                .expect("Channel token should be whitelisted in config!");
            let channel_context = channel_chain.with_channel(DUMMY_CAMPAIGN.channel);

            // make sure that the Channel is created in Database for the Accounting to work properly
            insert_channel(&database.pool, &channel_context)
                .await
                .expect("It should insert Channel");

            let publisher = *PUBLISHER;

            let leader = campaign.leader().unwrap();
            let follower = campaign.follower().unwrap();
            let payout = UnifiedNum::from_whole(0.03);

            // No Campaign Remaining set, should error
            {
                let spend_event = spend_for_event(
                    &database.pool,
                    &campaign_remaining,
                    &campaign,
                    publisher,
                    leader,
                    follower,
                    payout,
                )
                .await;

                assert!(
                    matches!(
                        &spend_event,
                        Err(Error::Event(
                            EventError::CampaignRemainingNotEnoughForPayout
                        ))
                    ),
                    "Campaign budget has no remaining funds to spend, result: {spend_event:?}"
                );
            }

            // Repeat the same call, but set the Campaign remaining budget in Redis
            {
                // 0.11 budget left
                set_campaign_remaining(&mut redis, campaign.id, 11_000_000).await;

                let spend_event = spend_for_event(
                    &database.pool,
                    &campaign_remaining,
                    &campaign,
                    publisher,
                    leader,
                    follower,
                    payout,
                )
                .await;

                assert!(
                    spend_event.is_ok(),
                    "Campaign budget has no remaining funds to spend or an error occurred"
                );

                // Payout: 0.03
                // Leader fee: 0.03
                // Leader payout: 0.03 * 0.03 / 1000.0 = 0.00 000 090 = UnifiedNum(90)
                //
                // Follower fee: 0.02
                // Follower payout: 0.03 * 0.02 / 1000.0 = 0.00 000 060 = UnifiedNum(60)

                // campaign budget left - payout - leader fee - follower fee
                // 0.11 - 0.03 - 0.00 000 090 - 0.00 000 060 = 0.07999850
                assert_eq!(
                    Some(7_999_850_i64),
                    campaign_remaining
                        .get_remaining_opt(campaign.id)
                        .await
                        .expect("Should have key")
                )
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::{
        update_campaign::{get_delta_budget, modify_campaign, DeltaBudget},
        *,
    };
    use crate::{
        db::{fetch_campaign, redis_pool::TESTS_POOL},
        test_util::setup_dummy_app,
    };
    use adapter::primitives::Deposit;
    use chrono::{TimeZone, Utc};
    use hyper::StatusCode;
    use primitives::{
        campaign::validators::Validators,
        config::GANACHE_CONFIG,
        sentry::campaign_list::{CampaignListResponse, ValidatorParam},
        test_util::{
            CREATOR, DUMMY_CAMPAIGN, DUMMY_VALIDATOR_FOLLOWER, DUMMY_VALIDATOR_LEADER, FOLLOWER,
            GUARDIAN, IDS, LEADER, LEADER_2, PUBLISHER_2,
        },
        unified_num::FromWhole,
        ValidatorDesc, ValidatorId,
    };

    #[tokio::test]
    /// Test single campaign creation and modification
    // &
    /// Test with multiple campaigns (because of Budget) and modifications
    async fn create_and_modify_with_multiple_campaigns() {
        let app = setup_dummy_app().await;

        // Create a new Campaign with different CampaignId
        let dummy_channel = DUMMY_CAMPAIGN.channel;
        let channel_chain = app
            .config
            .find_chain_of(dummy_channel.token)
            .expect("Channel token should be whitelisted in config!");
        let channel_context = channel_chain.with_channel(dummy_channel);

        // Set the deposit for the CREATOR use for all campaigns in the test
        assert_eq!(*CREATOR, DUMMY_CAMPAIGN.creator);
        app.adapter.client.set_deposit(
            &channel_context,
            *CREATOR,
            Deposit {
                // a deposit 4 times larger than the first Campaign.budget = 500
                // I.e. 2 000 TOKENS
                total: UnifiedNum::from_whole(2_000)
                    .to_precision(channel_context.token.precision.get()),
            },
        );

        let build_request = |create_campaign: CreateCampaign| -> Request<Body> {
            let auth = Auth {
                era: 0,
                uid: ValidatorId::from(create_campaign.creator),
                chain: channel_context.chain.clone(),
            };

            let body =
                Body::from(serde_json::to_string(&create_campaign).expect("Should serialize"));

            Request::builder()
                .extension(auth)
                .body(body)
                .expect("Should build Request")
        };

        let campaign_context: ChainOf<Campaign> = {
            // erases the CampaignId for the CreateCampaign request
            let mut create = CreateCampaign::from_campaign_erased(DUMMY_CAMPAIGN.clone(), None);
            create.budget = UnifiedNum::from_whole(500);

            let create_response = create_campaign(build_request(create), &app)
                .await
                .expect("Should create campaign");

            assert_eq!(StatusCode::OK, create_response.status());
            let json = hyper::body::to_bytes(create_response.into_body())
                .await
                .expect("Should get json");

            let campaign: Campaign =
                serde_json::from_slice(&json).expect("Should get new Campaign");

            assert_ne!(DUMMY_CAMPAIGN.id, campaign.id);

            let campaign_remaining = CampaignRemaining::new(app.redis.clone());

            let remaining = campaign_remaining
                .get_remaining_opt(campaign.id)
                .await
                .expect("Should get remaining from redis")
                .expect("There should be value for the Campaign");

            assert_eq!(
                UnifiedNum::from(50_000_000_000),
                UnifiedNum::from(remaining.unsigned_abs())
            );

            channel_context.clone().with(campaign)
        };

        // modify campaign
        // Deposit = 2 000
        // old Campaign.budget = 500
        // new Campaign.budget = 1 000
        // Deposit left = 1 000
        let modified = {
            let new_budget = UnifiedNum::from_whole(1000);
            let modify = ModifyCampaign {
                budget: Some(new_budget),
                validators: None,
                title: Some("Updated title".to_string()),
                pricing_bounds: None,
                event_submission: None,
                ad_units: None,
                targeting_rules: None,
            };

            let modified_campaign = modify_campaign(
                app.adapter.clone(),
                &app.pool,
                &app.config,
                &app.campaign_remaining,
                &campaign_context,
                modify,
            )
            .await
            .expect("Should modify campaign");

            assert_eq!(new_budget, modified_campaign.budget);
            assert_eq!(Some("Updated title".to_string()), modified_campaign.title);

            channel_context.clone().with(modified_campaign)
        };

        // we have 1000 left from our deposit, so we are using half of it
        // remaining Deposit = 1 000
        // new Campaign.budget = 500
        // Deposit left = 500
        let _second_campaign = {
            // erases the CampaignId for the CreateCampaign request
            let mut create_second =
                CreateCampaign::from_campaign_erased(DUMMY_CAMPAIGN.clone(), None);
            create_second.budget = UnifiedNum::from_whole(500);

            let create_response = create_campaign(build_request(create_second), &app)
                .await
                .expect("Should create campaign");

            assert_eq!(StatusCode::OK, create_response.status());
            let json = hyper::body::to_bytes(create_response.into_body())
                .await
                .expect("Should get json");

            let second_campaign: Campaign =
                serde_json::from_slice(&json).expect("Should get new Campaign");

            second_campaign
        };

        // No budget left for new campaigns
        // remaining Deposit = 500
        // new Campaign.budget = 600
        {
            // erases the CampaignId for the CreateCampaign request
            let mut create = CreateCampaign::from_campaign_erased(DUMMY_CAMPAIGN.clone(), None);
            create.budget = UnifiedNum::from_whole(600);

            let create_err = create_campaign(build_request(create), &app)
                .await
                .expect_err("Should return Error response");

            assert_eq!(
                ResponseError::BadRequest(
                    "Not enough deposit left for the new campaign's budget".to_string()
                ),
                create_err
            );
        }

        // modify first campaign, by lowering the budget from 1000 to 900
        let modified = {
            let lower_budget = UnifiedNum::from_whole(900);
            let modify = ModifyCampaign {
                budget: Some(lower_budget),
                validators: None,
                title: None,
                pricing_bounds: None,
                event_submission: None,
                ad_units: None,
                targeting_rules: None,
            };

            let modified_campaign = modify_campaign(
                app.adapter.clone(),
                &app.pool,
                &app.config,
                &app.campaign_remaining,
                &modified,
                modify,
            )
            .await
            .expect("Should modify campaign");

            assert_eq!(lower_budget, modified_campaign.budget);

            modified.clone().with(modified_campaign)
        };

        // Just enough budget to create this Campaign
        // remaining Deposit = 600
        // new Campaign.budget = 600
        {
            // erases the CampaignId for the CreateCampaign request
            let mut create = CreateCampaign::from_campaign_erased(DUMMY_CAMPAIGN.clone(), None);
            create.budget = UnifiedNum::from_whole(600);

            let create_response = create_campaign(build_request(create), &app)
                .await
                .expect("Should return create campaign");

            let json = hyper::body::to_bytes(create_response.into_body())
                .await
                .expect("Should get json");

            let _campaign: Campaign =
                serde_json::from_slice(&json).expect("Should get new Campaign");
        }

        // Modify a campaign without enough budget
        // remaining Deposit = 0
        // new Campaign.budget = 1100
        // current Campaign.budget = 900
        {
            let new_budget = UnifiedNum::from_whole(1_100);
            let modify = ModifyCampaign {
                budget: Some(new_budget),
                validators: None,
                title: None,
                pricing_bounds: None,
                event_submission: None,
                ad_units: None,
                targeting_rules: None,
            };

            let modify_err = modify_campaign(
                app.adapter.clone(),
                &app.pool,
                &app.config,
                &app.campaign_remaining,
                &modified,
                modify,
            )
            .await
            .expect_err("Should return Error response");

            assert!(
                matches!(&modify_err, Error::NewBudget(string) if string == "Not enough deposit left for the campaign's new budget"),
                "Found error: {modify_err}"
            );
        }
    }

    #[tokio::test]
    async fn delta_budgets_are_calculated_correctly() {
        let redis = TESTS_POOL.get().await.expect("Should return Object");
        let campaign_remaining = CampaignRemaining::new(redis.connection.clone());

        let campaign = DUMMY_CAMPAIGN.clone();

        // Equal budget
        {
            let delta_budget = get_delta_budget(&campaign_remaining, &campaign, campaign.budget)
                .await
                .expect("should get delta budget");
            assert!(delta_budget.is_none());
        }
        // Spent cant be higher than the new budget
        {
            campaign_remaining
                .set_initial(campaign.id, UnifiedNum::from_whole(600))
                .await
                .expect("should set");

            // campaign_spent > new_budget
            let new_budget = UnifiedNum::from_whole(300);
            let delta_budget = get_delta_budget(&campaign_remaining, &campaign, new_budget).await;

            assert!(
                matches!(&delta_budget, Err(Error::NewBudget(_))),
                "Got result: {delta_budget:?}"
            );

            // campaign_spent == new_budget
            let new_budget = UnifiedNum::from_whole(400);
            let delta_budget = get_delta_budget(&campaign_remaining, &campaign, new_budget).await;

            assert!(
                matches!(&delta_budget, Err(Error::NewBudget(_))),
                "Got result: {delta_budget:?}"
            );
        }
        // Increasing budget
        {
            campaign_remaining
                .set_initial(campaign.id, UnifiedNum::from_whole(900))
                .await
                .expect("should set");
            let new_budget = UnifiedNum::from_whole(1_100);
            let delta_budget = get_delta_budget(&campaign_remaining, &campaign, new_budget)
                .await
                .expect("should get delta budget");
            assert!(delta_budget.is_some());
            let increase_by = UnifiedNum::from_whole(100);

            assert_eq!(delta_budget, Some(DeltaBudget::Increase(increase_by)));
        }
        // Decreasing budget
        {
            campaign_remaining
                .set_initial(campaign.id, UnifiedNum::from_whole(900))
                .await
                .expect("should set");
            let new_budget = UnifiedNum::from_whole(800);
            let delta_budget = get_delta_budget(&campaign_remaining, &campaign, new_budget)
                .await
                .expect("should get delta budget");
            assert!(delta_budget.is_some());
            let decrease_by = UnifiedNum::from_whole(200);

            assert_eq!(delta_budget, Some(DeltaBudget::Decrease(decrease_by)));
        }
    }

    #[tokio::test]
    async fn campaign_is_closed_properly() {
        // create a new campaign with a new CampaignId
        let campaign =
            CreateCampaign::from_campaign_erased(DUMMY_CAMPAIGN.clone(), None).into_campaign();

        let app = setup_dummy_app().await;

        let channel_chain = app
            .config
            .find_chain_of(DUMMY_CAMPAIGN.channel.token)
            .expect("Channel token should be whitelisted in config!");
        let channel_context = channel_chain.with_channel(DUMMY_CAMPAIGN.channel);

        insert_channel(&app.pool, &channel_context)
            .await
            .expect("Should insert dummy channel");
        insert_campaign(&app.pool, &campaign)
            .await
            .expect("Should insert dummy campaign");

        let campaign_context = app
            .config
            .find_chain_of(campaign.channel.token)
            .expect("Config should have the Dummy campaign.channel.token")
            .with(campaign.clone());

        // Test if remaining is set to 0
        {
            app.campaign_remaining
                .set_initial(campaign.id, campaign.budget)
                .await
                .expect("should set");

            let auth = Auth {
                era: 0,
                uid: ValidatorId::from(campaign.creator),
                chain: campaign_context.chain.clone(),
            };

            let req = Request::builder()
                .extension(auth)
                .extension(campaign_context.clone())
                .body(Body::empty())
                .expect("Should build Request");

            close_campaign(req, &app)
                .await
                .expect("Should close campaign");

            let closed_campaign = fetch_campaign(app.pool.clone(), &campaign.id)
                .await
                .expect("Should fetch campaign")
                .expect("Campaign should exist");

            // remaining == campaign_budget therefore old_budget - remaining = 0
            assert_eq!(closed_campaign.budget, UnifiedNum::from_u64(0));

            let remaining = app
                .campaign_remaining
                .get_remaining_opt(campaign.id)
                .await
                .expect("Should get remaining from redis")
                .expect("There should be value for the Campaign");

            assert_eq!(remaining, 0);
        }

        // Test if an error is returned when request is not sent by creator
        {
            let auth = Auth {
                era: 0,
                uid: IDS[&LEADER],
                chain: campaign_context.chain.clone(),
            };

            let req = Request::builder()
                .extension(auth)
                .extension(campaign_context.clone())
                .body(Body::empty())
                .expect("Should build Request");

            let res = close_campaign(req, &app)
                .await
                .expect_err("Should return error for Bad Campaign");

            assert_eq!(
                ResponseError::Forbidden("Request not sent by campaign creator".to_string()),
                res
            );
        }
    }

    async fn res_to_campaign_list_response(res: Response<Body>) -> CampaignListResponse {
        let json = hyper::body::to_bytes(res.into_body())
            .await
            .expect("Should get json");

        serde_json::from_slice(&json).expect("Should deserialize CampaignListResponse")
    }

    #[tokio::test]
    async fn test_campaign_list() {
        let mut app = setup_dummy_app().await;
        app.config.campaigns_find_limit = 2;
        // Setting up new leader and a channel and campaign which use it on Ganache #1337
        let dummy_leader_2 = ValidatorDesc {
            id: IDS[&LEADER_2],
            url: "http://tom.adex.network".to_string(),
            fee: 200.into(),
            fee_addr: None,
        };
        let channel_new_leader = Channel {
            leader: IDS[&*LEADER_2],
            follower: IDS[&*FOLLOWER],
            guardian: *GUARDIAN,
            token: DUMMY_CAMPAIGN.channel.token,
            nonce: DUMMY_CAMPAIGN.channel.nonce,
        };
        let mut campaign_new_leader = DUMMY_CAMPAIGN.clone();
        campaign_new_leader.id = CampaignId::new();
        campaign_new_leader.channel = channel_new_leader;
        campaign_new_leader.validators =
            Validators::new((dummy_leader_2.clone(), DUMMY_VALIDATOR_FOLLOWER.clone()));
        campaign_new_leader.created = Utc.ymd(2021, 2, 1).and_hms(8, 0, 0);

        let chain_1_token = GANACHE_CONFIG.chains["Ganache #1"].tokens["Mocked TOKEN 1"].address;
        // Setting up new follower and a channel and campaign which use it on Ganache #1
        let dummy_follower_2 = ValidatorDesc {
            id: IDS[&GUARDIAN],
            url: "http://jerry.adex.network".to_string(),
            fee: 300.into(),
            fee_addr: None,
        };
        let channel_new_follower = Channel {
            leader: IDS[&*LEADER],
            follower: IDS[&*GUARDIAN],
            guardian: *GUARDIAN,
            token: chain_1_token,
            nonce: DUMMY_CAMPAIGN.channel.nonce,
        };
        let mut campaign_new_follower = DUMMY_CAMPAIGN.clone();
        campaign_new_follower.id = CampaignId::new();
        campaign_new_follower.channel = channel_new_follower;
        campaign_new_follower.validators =
            Validators::new((DUMMY_VALIDATOR_LEADER.clone(), dummy_follower_2.clone()));
        campaign_new_follower.created = Utc.ymd(2021, 2, 1).and_hms(9, 0, 0);

        // Setting up a channel and campaign which use the new leader and follower on Ganache #1
        let channel_new_leader_and_follower = Channel {
            leader: IDS[&*LEADER_2],
            follower: IDS[&*GUARDIAN],
            guardian: *GUARDIAN,
            token: chain_1_token,
            nonce: DUMMY_CAMPAIGN.channel.nonce,
        };
        let mut campaign_new_leader_and_follower = DUMMY_CAMPAIGN.clone();
        campaign_new_leader_and_follower.id = CampaignId::new();
        campaign_new_leader_and_follower.channel = channel_new_leader_and_follower;
        campaign_new_leader_and_follower.validators =
            Validators::new((dummy_leader_2.clone(), dummy_follower_2.clone()));
        campaign_new_leader_and_follower.created = Utc.ymd(2021, 2, 1).and_hms(10, 0, 0);
        let channel_chain = app
            .config
            .find_chain_of(DUMMY_CAMPAIGN.channel.token)
            .expect("Channel token should be whitelisted in config!");

        insert_channel(
            &app.pool,
            &channel_chain.clone().with_channel(DUMMY_CAMPAIGN.channel),
        )
        .await
        .expect("Should insert dummy channel");
        insert_campaign(&app.pool, &DUMMY_CAMPAIGN)
            .await
            .expect("Should insert dummy campaign");
        insert_channel(
            &app.pool,
            &channel_chain.clone().with_channel(channel_new_leader),
        )
        .await
        .expect("Should insert dummy channel");
        insert_campaign(&app.pool, &campaign_new_leader)
            .await
            .expect("Should insert dummy campaign");
        insert_channel(
            &app.pool,
            &channel_chain.clone().with_channel(channel_new_follower),
        )
        .await
        .expect("Should insert dummy channel");
        insert_campaign(&app.pool, &campaign_new_follower)
            .await
            .expect("Should insert dummy campaign");
        insert_channel(
            &app.pool,
            &channel_chain.with_channel(channel_new_leader_and_follower),
        )
        .await
        .expect("Should insert dummy channel");
        insert_campaign(&app.pool, &campaign_new_leader_and_follower)
            .await
            .expect("Should insert dummy campaign");

        let mut campaign_other_creator = DUMMY_CAMPAIGN.clone();
        campaign_other_creator.id = CampaignId::new();
        campaign_other_creator.creator = *PUBLISHER_2;
        campaign_other_creator.created = Utc.ymd(2021, 2, 1).and_hms(11, 0, 0);

        insert_campaign(&app.pool, &campaign_other_creator)
            .await
            .expect("Should insert dummy campaign");

        let mut campaign_long_active_to = DUMMY_CAMPAIGN.clone();
        campaign_long_active_to.id = CampaignId::new();
        campaign_long_active_to.active.to = Utc.ymd(2101, 1, 30).and_hms(0, 0, 0);
        campaign_long_active_to.created = Utc.ymd(2021, 2, 1).and_hms(12, 0, 0);

        insert_campaign(&app.pool, &campaign_long_active_to)
            .await
            .expect("Should insert dummy campaign");

        let build_request = |query: CampaignListQuery| {
            let query = serde_qs::to_string(&query).expect("should parse query");
            Request::builder()
                .uri(format!("http://127.0.0.1/v5/campaign/list?{}", query))
                .body(Body::empty())
                .expect("Should build Request")
        };

        // Test for dummy leader
        {
            let query = CampaignListQuery {
                page: 0,
                active_to_ge: Utc::now(),
                creator: None,
                validator: Some(ValidatorParam::Leader(DUMMY_VALIDATOR_LEADER.id)),
            };
            let res = campaign_list(build_request(query), &app)
                .await
                .expect("should get campaigns");
            let res = res_to_campaign_list_response(res).await;

            assert_eq!(
                res.campaigns,
                vec![DUMMY_CAMPAIGN.clone(), campaign_new_follower.clone()],
                "First page of campaigns with dummy leader is correct"
            );
            assert_eq!(res.pagination.total_pages, 2);

            let query = CampaignListQuery {
                page: 1,
                active_to_ge: Utc::now(),
                creator: None,
                validator: Some(ValidatorParam::Leader(DUMMY_VALIDATOR_LEADER.id)),
            };
            let res = campaign_list(build_request(query), &app)
                .await
                .expect("should get campaigns");
            let res = res_to_campaign_list_response(res).await;

            assert_eq!(
                res.campaigns,
                vec![
                    campaign_other_creator.clone(),
                    campaign_long_active_to.clone()
                ],
                "Second page of campaigns with dummy leader is correct"
            );
        }

        // Test for dummy follower
        {
            let query = CampaignListQuery {
                page: 0,
                active_to_ge: Utc::now(),
                creator: None,
                validator: Some(ValidatorParam::Validator(DUMMY_VALIDATOR_FOLLOWER.id)),
            };
            let res = campaign_list(build_request(query), &app)
                .await
                .expect("should get campaigns");
            let res = res_to_campaign_list_response(res).await;

            assert_eq!(
                res.campaigns,
                vec![DUMMY_CAMPAIGN.clone(), campaign_new_leader.clone()],
                "First page of campaigns with dummy follower is correct"
            );
            assert_eq!(res.pagination.total_pages, 2);

            let query = CampaignListQuery {
                page: 1,
                active_to_ge: Utc::now(),
                creator: None,
                validator: Some(ValidatorParam::Validator(DUMMY_VALIDATOR_FOLLOWER.id)),
            };
            let res = campaign_list(build_request(query), &app)
                .await
                .expect("should get campaigns");
            let res = res_to_campaign_list_response(res).await;

            assert_eq!(
                res.campaigns,
                vec![
                    campaign_other_creator.clone(),
                    campaign_long_active_to.clone()
                ],
                "Second page of campaigns with dummy follower is correct"
            );
        }

        // Test for dummy leader 2
        {
            let query = CampaignListQuery {
                page: 0,
                active_to_ge: Utc::now(),
                creator: None,
                validator: Some(ValidatorParam::Leader(dummy_leader_2.id)),
            };
            let res = campaign_list(build_request(query), &app)
                .await
                .expect("should get campaigns");
            let res = res_to_campaign_list_response(res).await;

            assert_eq!(
                res.campaigns,
                vec![
                    campaign_new_leader.clone(),
                    campaign_new_leader_and_follower.clone()
                ],
                "Campaigns with dummy leader 2 are correct"
            );
            assert_eq!(res.pagination.total_pages, 1);
        }

        // Test for dummy follower 2
        {
            let query = CampaignListQuery {
                page: 0,
                active_to_ge: Utc::now(),
                creator: None,
                validator: Some(ValidatorParam::Validator(dummy_follower_2.id)),
            };
            let res = campaign_list(build_request(query), &app)
                .await
                .expect("should get campaigns");
            let res = res_to_campaign_list_response(res).await;

            assert_eq!(
                res.campaigns,
                vec![
                    campaign_new_follower.clone(),
                    campaign_new_leader_and_follower.clone()
                ],
                "Campaigns with dummy follower 2 are correct"
            );
            assert_eq!(res.pagination.total_pages, 1);
        }

        // Test for other creator
        {
            let query = CampaignListQuery {
                page: 0,
                active_to_ge: Utc::now(),
                creator: Some(*PUBLISHER_2),
                validator: None,
            };
            let res = campaign_list(build_request(query), &app)
                .await
                .expect("should get campaigns");
            let res = res_to_campaign_list_response(res).await;

            assert_eq!(
                res.campaigns,
                vec![campaign_other_creator.clone()],
                "The campaign with a different creator is retrieved correctly"
            );
            assert_eq!(res.pagination.total_pages, 1);
        }

        // Test for active_to
        {
            let query = CampaignListQuery {
                page: 0,
                active_to_ge: Utc.ymd(2101, 1, 1).and_hms(0, 0, 0),
                creator: None,
                validator: None,
            };
            let res = campaign_list(build_request(query), &app)
                .await
                .expect("should get campaigns");
            let res = res_to_campaign_list_response(res).await;

            assert_eq!(
                res.campaigns,
                vec![campaign_long_active_to.clone()],
                "The campaign with a longer active_to is retrieved correctly"
            );
            assert_eq!(res.pagination.total_pages, 1);
        }
    }
}
