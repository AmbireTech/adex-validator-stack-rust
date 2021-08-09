use crate::{
    access::{self, check_access},
    db::{
        accounting::get_accounting_spent,
        campaign::{get_campaigns_by_channel, insert_campaign, update_campaign},
        spendable::fetch_spendable,
        CampaignRemaining, DbPool, RedisError,
    },
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
    Address, Campaign, UnifiedNum,
};
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
        .map_err(|err| ResponseError::FailedValidation(err.to_string()))?;

    if auth.uid.to_address() != campaign.creator {
        return Err(ResponseError::Forbidden(
            "Request not sent by campaign creator".to_string(),
        ));
    }

    let error_response =
        ResponseError::BadRequest("err occurred; please try again later".to_string());

    let total_remaining =
        {
            let accounting_spent =
                get_accounting_spent(app.pool.clone(), &campaign.creator, &campaign.channel.id())
                    .await?;

            let latest_spendable =
                fetch_spendable(app.pool.clone(), &campaign.creator, &campaign.channel.id())
                    .await?
                    .ok_or(ResponseError::BadRequest(
                        "No spendable amount found for the Campaign creator".to_string(),
                    ))?;
            // Gets the latest Spendable for this (spender, channelId) pair
            let total_deposited = latest_spendable.deposit.total;

            total_deposited.checked_sub(&accounting_spent).ok_or(
                ResponseError::FailedValidation("No more budget remaining".to_string()),
            )?
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

    if !(campaigns_remaining_sum <= total_remaining) || campaign.budget > total_remaining {
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

pub mod update_campaign {
    use crate::db::CampaignRemaining;

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
            let channel_campaigns = get_campaigns_by_channel(&pool, &campaign.channel.id())
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

            if !(new_campaigns_remaining <= total_remaining) {
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
    use super::{update_campaign::modify_campaign, *};
    use crate::{
        db::{accounting::insert_accounting, spendable::insert_spendable},
        test_util::setup_dummy_app,
    };
    use hyper::StatusCode;
    use primitives::{
        sentry::accounting::{Balances, CheckedState},
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
                    // a deposit equal to double the Campaign Budget
                    total: UnifiedNum::from(200_000_000_000),
                    still_on_create2: UnifiedNum::from(0),
                },
            };
            assert!(insert_spendable(app.pool.clone(), &spendable)
                .await
                .expect("Should insert Spendable for Campaign creator"));

            let mut balances = Balances::<CheckedState>::default();
            balances.add_spender(create.creator);

            // TODO: Replace this once https://github.com/AdExNetwork/adex-validator-stack-rust/pull/413 is merged
            let _accounting = insert_accounting(app.pool.clone(), create.channel.clone(), balances)
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

            assert_eq!(ResponseError::BadRequest("Not enough deposit left for the new campaign's budget".to_string()), create_err);
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

            let modify_err =
                modify_campaign(&app.pool, &app.campaign_remaining, modified, modify)
                    .await
                    .expect_err("Should return Error response");

            assert!(matches!(modify_err, Error::NewBudget(string) if string == "Not enough deposit left for the campaign's new budget"));
        }
    }
}
