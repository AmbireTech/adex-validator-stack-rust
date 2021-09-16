use crate::{
    db::{
        accounting::{get_accounting, Side},
        campaign::{get_campaigns_by_channel, insert_campaign, list_campaigns, update_campaign},
        spendable::fetch_spendable,
        CampaignRemaining, DbPool, RedisError,
    },
    success_response, Application, Auth, ResponseError,
};
use deadpool_postgres::PoolError;
use hyper::{Body, Request, Response};
use primitives::{
    adapter::Adapter,
    campaign_validator::Validator,
    sentry::campaign::CampaignListQuery,
    sentry::campaign_create::{CreateCampaign, ModifyCampaign},
    Address, Campaign, UnifiedNum,
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
        .map_err(|err| ResponseError::FailedValidation(err.to_string()))?;

    if auth.uid.to_address() != campaign.creator {
        return Err(ResponseError::Forbidden(
            "Request not sent by campaign creator".to_string(),
        ));
    }

    let error_response =
        ResponseError::BadRequest("err occurred; please try again later".to_string());

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

        let latest_spendable =
            fetch_spendable(app.pool.clone(), &campaign.creator, &campaign.channel.id())
                .await?
                .ok_or_else(|| {
                    ResponseError::BadRequest(
                        "No spendable amount found for the Campaign creator".to_string(),
                    )
                })?;
        // Gets the latest Spendable for this (spender, channelId) pair
        let total_deposited = latest_spendable.deposit.total;

        total_deposited
            .checked_sub(&accounting_spent)
            .ok_or_else(|| {
                ResponseError::FailedValidation("No more budget remaining".to_string())
            })?
    };

    let channel_campaigns = get_campaigns_by_channel(&app.pool, &campaign.channel.id())
        .await?
        .iter()
        .map(|c| c.id)
        .collect::<Vec<_>>();

    let campaigns_remaining_sum = app
        .campaign_remaining
        .get_multiple(&channel_campaigns)
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

pub async fn campaign_list<A: Adapter>(
    req: Request<Body>,
    app: &Application<A>,
) -> Result<Response<Body>, ResponseError> {
    let query = serde_urlencoded::from_str::<CampaignListQuery>(req.uri().query().unwrap_or(""))?;

    let limit = 100; // TODO: Use a value from config
    let skip = query
        .page
        .checked_mul(limit.into())
        .ok_or_else(|| ResponseError::BadRequest("Page and/or limit is too large".into()))?;
    let list_response = list_campaigns(
        &app.pool,
        skip,
        limit,
        &query.creator,
        &query.validator,
        &query.is_leader,
        &query.active_to_ge,
    )
    .await?;

    Ok(success_response(serde_json::to_string(&list_response)?))
}

pub mod update_campaign {
    use crate::db::{accounting::Side, CampaignRemaining};

    use super::*;

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
            &app.campaign_remaining,
            campaign_being_mutated,
            modify_campaign_fields,
        )
        .await
        .map_err(|err| ResponseError::BadRequest(err.to_string()))?;

        Ok(success_response(serde_json::to_string(&modified_campaign)?))
    }

    pub async fn modify_campaign(
        pool: &DbPool,
        campaign_remaining: &CampaignRemaining,
        campaign: Campaign,
        modify_campaign: ModifyCampaign,
    ) -> Result<Campaign, Error> {
        // *NOTE*: When updating campaigns make sure sum(campaigns.map(getRemaining)) <= totalDepoisted - totalspent
        // !WARNING!: totalSpent != sum(campaign.map(c => c.spending)) therefore we must always calculate remaining funds based on total_deposit - lastApprovedNewState.spenders[user]
        // *NOTE*: To close a campaign set campaignBudget to campaignSpent so that spendable == 0

        let delta_budget = if let Some(new_budget) = modify_campaign.budget {
            get_delta_budget(campaign_remaining, &campaign, new_budget).await?
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

            let latest_spendable =
                fetch_spendable(pool.clone(), &campaign.creator, &campaign.channel.id())
                    .await?
                    .ok_or(Error::SpenderNotFound(campaign.creator))?;

            // Gets the latest Spendable for this (spender, channelId) pair
            let total_deposited = latest_spendable.deposit.total;

            let total_remaining = total_deposited
                .checked_sub(&accounting_spent)
                .ok_or(Error::Calculation)?;
            let channel_campaigns = get_campaigns_by_channel(pool, &campaign.channel.id())
                .await?
                .iter()
                .map(|c| c.id)
                .collect::<Vec<_>>();

            // this will include the Campaign we are currently modifying
            let campaigns_current_remaining_sum = campaign_remaining
                .get_multiple(&channel_campaigns)
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

        let modified_campaign = modify_campaign.apply(campaign);
        update_campaign(pool, &modified_campaign).await?;

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
        db::{accounting::spend_amount, CampaignRemaining, DbPool, PoolError, RedisError},
        payout::get_payout,
        spender::fee::calculate_fee,
        Application, Auth, ResponseError, Session,
    };
    use hyper::{Body, Request, Response};
    use primitives::{
        adapter::Adapter,
        balances::{Balances, CheckedState, OverflowError},
        sentry::{Event, SuccessResponse},
        Address, Campaign, CampaignId, DomainError, UnifiedNum, ValidatorDesc,
    };
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

    pub async fn handle_route<A: Adapter + 'static>(
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
        // handle events - check access
        check_access(
            &app.redis,
            session,
            auth,
            &app.config.ip_rate_limit,
            campaign,
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
            let result: Result<Option<()>, Error> = {
                // calculate earners payouts
                let payout = get_payout(&app.logger, campaign, &event, session)?;

                match payout {
                    Some((earner, payout)) => spend_for_event(
                        &app.pool,
                        &app.campaign_remaining,
                        campaign,
                        earner,
                        leader,
                        follower,
                        payout,
                    )
                    .await
                    .map(Some),
                    None => Ok(None),
                }
            };

            events_success.push((event, result));
        }

        // TODO AIP#61 - aggregate Events and put into analytics

        Ok(true)
    }

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
        use primitives::util::tests::prep_db::{ADDRESSES, DUMMY_CAMPAIGN};
        use redis::aio::MultiplexedConnection;

        use crate::db::{
            redis_pool::TESTS_POOL,
            tests_postgres::{setup_test_migrations, DATABASE_POOL},
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

            setup_test_migrations(database.pool.clone())
                .await
                .expect("Migrations should succeed");

            let campaign = DUMMY_CAMPAIGN.clone();

            let publisher = ADDRESSES["publisher"];

            let leader = campaign.leader().unwrap();
            let follower = campaign.follower().unwrap();
            let payout = UnifiedNum::from(300);

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
                        spend_event,
                        Err(Error::Event(
                            EventError::CampaignRemainingNotEnoughForPayout
                        ))
                    ),
                    "Campaign budget has no remaining funds to spend"
                );
            }

            // Repeat the same call, but set the Campaign remaining budget in Redis
            {
                set_campaign_remaining(&mut redis, campaign.id, 11_000).await;

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
                    "Campaign budget has no remaining funds to spend"
                );

                // Payout: 300
                // Leader fee: 100
                // Leader payout: 300 * 100 / 1000 = 30
                // Follower fee: 100
                // Follower payout: 300 * 100 / 1000 = 30
                assert_eq!(
                    Some(10_640_i64),
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
    use super::{update_campaign::modify_campaign, *};
    use crate::{
        db::{accounting::update_accounting, spendable::insert_spendable},
        test_util::setup_dummy_app,
    };
    use hyper::StatusCode;
    use primitives::{
        spender::{Deposit, Spendable},
        util::tests::prep_db::DUMMY_CAMPAIGN,
        ValidatorId,
    };

    #[tokio::test]
    /// Test single campaign creation and modification
    // &
    /// Test with multiple campaigns (because of Budget) a modification of campaign
    async fn create_and_modify_with_multiple_campaigns() {
        let app = setup_dummy_app().await;

        let build_request = |create_campaign: CreateCampaign| -> Request<Body> {
            let auth = Auth {
                era: 0,
                uid: ValidatorId::from(create_campaign.creator),
            };

            let body =
                Body::from(serde_json::to_string(&create_campaign).expect("Should serialize"));

            Request::builder()
                .extension(auth)
                .body(body)
                .expect("Should build Request")
        };

        let campaign: Campaign = {
            // erases the CampaignId for the CreateCampaign request
            let mut create = CreateCampaign::from(DUMMY_CAMPAIGN.clone());
            // 500.00000000
            create.budget = UnifiedNum::from(50_000_000_000);

            let spendable = Spendable {
                spender: create.creator,
                channel: create.channel.clone(),
                deposit: Deposit {
                    // a deposit 4 times larger than the Campaign Budget
                    total: UnifiedNum::from(200_000_000_000),
                    still_on_create2: UnifiedNum::from(0),
                },
            };
            assert!(insert_spendable(app.pool.clone(), &spendable)
                .await
                .expect("Should insert Spendable for Campaign creator"));

            let _accounting = update_accounting(
                app.pool.clone(),
                create.channel.id(),
                create.creator,
                Side::Spender,
                UnifiedNum::default(),
            )
            .await
            .expect("Should create Accounting");

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
            campaign
        };

        // modify campaign
        let modified = {
            // 1000.00000000
            let new_budget = UnifiedNum::from(100_000_000_000);
            let modify = ModifyCampaign {
                budget: Some(new_budget.clone()),
                validators: None,
                title: Some("Updated title".to_string()),
                pricing_bounds: None,
                event_submission: None,
                ad_units: None,
                targeting_rules: None,
            };

            let modified_campaign =
                modify_campaign(&app.pool, &app.campaign_remaining, campaign.clone(), modify)
                    .await
                    .expect("Should modify campaign");

            assert_eq!(new_budget, modified_campaign.budget);
            assert_eq!(Some("Updated title".to_string()), modified_campaign.title);

            modified_campaign
        };

        // we have 1000 left from our deposit, so we are using half of it
        let _second_campaign = {
            // erases the CampaignId for the CreateCampaign request
            let mut create_second = CreateCampaign::from(DUMMY_CAMPAIGN.clone());
            // 500.00000000
            create_second.budget = UnifiedNum::from(50_000_000_000);

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
        // remaining: 500
        // new campaign budget: 600
        {
            // erases the CampaignId for the CreateCampaign request
            let mut create = CreateCampaign::from(DUMMY_CAMPAIGN.clone());
            // 600.00000000
            create.budget = UnifiedNum::from(60_000_000_000);

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
            let lower_budget = UnifiedNum::from(90_000_000_000);
            let modify = ModifyCampaign {
                budget: Some(lower_budget.clone()),
                validators: None,
                title: None,
                pricing_bounds: None,
                event_submission: None,
                ad_units: None,
                targeting_rules: None,
            };

            let modified_campaign =
                modify_campaign(&app.pool, &app.campaign_remaining, modified, modify)
                    .await
                    .expect("Should modify campaign");

            assert_eq!(lower_budget, modified_campaign.budget);

            modified_campaign
        };

        // Just enough budget to create this Campaign
        // remaining: 600
        // new campaign budget: 600
        {
            // erases the CampaignId for the CreateCampaign request
            let mut create = CreateCampaign::from(DUMMY_CAMPAIGN.clone());
            // 600.00000000
            create.budget = UnifiedNum::from(60_000_000_000);

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
        // remaining: 0
        // new campaign budget: 1100
        // current campaign budget: 900
        {
            let new_budget = UnifiedNum::from(110_000_000_000);
            let modify = ModifyCampaign {
                budget: Some(new_budget),
                validators: None,
                title: None,
                pricing_bounds: None,
                event_submission: None,
                ad_units: None,
                targeting_rules: None,
            };

            let modify_err = modify_campaign(&app.pool, &app.campaign_remaining, modified, modify)
                .await
                .expect_err("Should return Error response");

            assert!(
                matches!(modify_err, Error::NewBudget(string) if string == "Not enough deposit left for the campaign's new budget")
            );
        }
    }
}
