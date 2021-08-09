use crate::{success_response, Application, ResponseError};
use hyper::{Body, Request, Response};
use primitives::{
    adapter::Adapter,
    sentry::{campaign_create::CreateCampaign, SuccessResponse},
};

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

    let error_response = ResponseError::BadRequest("err occurred; please try again later".into());

    // insert Campaign

    // match insert_campaign(&app.pool, &campaign).await {
    //     Err(error) => {
    //         error!(&app.logger, "{}", &error; "module" => "create_channel");

    //         match error {
    //             PoolError::Backend(error) if error.code() == Some(&SqlState::UNIQUE_VIOLATION) => {
    //                 Err(ResponseError::Conflict(
    //                     "channel already exists".to_string(),
    //                 ))
    //             }
    //             _ => Err(error_response),
    //         }
    //     }
    //     Ok(false) => Err(error_response),
    //     _ => Ok(()),
    // }?;

    let create_response = SuccessResponse { success: true };

    Ok(success_response(serde_json::to_string(&campaign)?))
}

pub mod insert_events {

    use std::collections::HashMap;

    use crate::{
        access::{self, check_access},
        db::{accounting::spend_amount, DbPool, PoolError, RedisError},
        payout::get_payout,
        spender::fee::calculate_fee,
        Application, Auth, ResponseError, Session,
    };
    use hyper::{Body, Request, Response};
    use primitives::{
        adapter::Adapter,
        sentry::{
            accounting::{Balances, CheckedState, OverflowError},
            Event, SuccessResponse,
        },
        Address, Campaign, CampaignId, DomainError, UnifiedNum, ValidatorDesc,
    };
    use redis::aio::MultiplexedConnection;
    use thiserror::Error;

    // TODO AIP#61: Use the Campaign Modify const here
    pub const CAMPAIGN_REMAINING_KEY: &str = "campaignRemaining";

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
                        app.redis.clone(),
                        &campaign,
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
        mut redis: MultiplexedConnection,
        campaign: &Campaign,
        earner: Address,
        leader: &ValidatorDesc,
        follower: &ValidatorDesc,
        amount: UnifiedNum,
    ) -> Result<(), Error> {
        // distribute fees
        let leader_fee =
            calculate_fee((earner, amount), &leader).map_err(EventError::FeeCalculation)?;
        let follower_fee =
            calculate_fee((earner, amount), &follower).map_err(EventError::FeeCalculation)?;

        // First update redis `campaignRemaining:{CampaignId}` key
        let spending = [amount, leader_fee, follower_fee]
            .iter()
            .sum::<Option<UnifiedNum>>()
            .ok_or(EventError::EventPayoutOverflow)?;

        if !has_enough_remaining_budget(&mut redis, campaign.id, spending).await? {
            return Err(Error::Event(
                EventError::CampaignRemainingNotEnoughForPayout,
            ));
        }

        // The event payout decreases the remaining budget for the Campaign
        let remaining = decrease_remaining_budget(&mut redis, campaign.id, spending).await?;

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
        redis: &mut MultiplexedConnection,
        campaign: CampaignId,
        amount: UnifiedNum,
    ) -> Result<bool, RedisError> {
        let key = format!("{}:{}", CAMPAIGN_REMAINING_KEY, campaign);

        let remaining = redis::cmd("GET")
            .arg(&key)
            .query_async::<_, Option<i64>>(redis)
            .await?
            .unwrap_or_default();

        Ok(remaining > 0 && remaining.unsigned_abs() > amount.to_u64())
    }

    async fn decrease_remaining_budget(
        redis: &mut MultiplexedConnection,
        campaign: CampaignId,
        amount: UnifiedNum,
    ) -> Result<i64, RedisError> {
        let key = format!("{}:{}", CAMPAIGN_REMAINING_KEY, campaign);

        let remaining = redis::cmd("DECRBY")
            .arg(&key)
            .arg(amount.to_u64())
            .query_async::<_, i64>(redis)
            .await?;

        Ok(remaining)
    }

    #[cfg(test)]
    mod test {
        use primitives::util::tests::prep_db::{ADDRESSES, DUMMY_CAMPAIGN};

        use crate::db::{
            redis_pool::TESTS_POOL,
            tests_postgres::{setup_test_migrations, DATABASE_POOL},
        };

        use super::*;

        /// Helper function to get the Campaign Remaining budget in Redis for the tests
        async fn get_campaign_remaining(
            redis: &mut MultiplexedConnection,
            campaign: CampaignId,
        ) -> Option<i64> {
            let key = format!("{}:{}", CAMPAIGN_REMAINING_KEY, campaign);

            redis::cmd("GET")
                .arg(&key)
                .query_async(redis)
                .await
                .expect("Should set Campaign remaining key")
        }

        /// Helper function to set the Campaign Remaining budget in Redis for the tests
        async fn set_campaign_remaining(
            redis: &mut MultiplexedConnection,
            campaign: CampaignId,
            remaining: i64,
        ) {
            let key = format!("{}:{}", CAMPAIGN_REMAINING_KEY, campaign);

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
            let campaign = DUMMY_CAMPAIGN.id;
            let amount = UnifiedNum::from(10_000);

            let no_remaining_budget_set = has_enough_remaining_budget(&mut redis, campaign, amount)
                .await
                .expect("Should check campaign remaining");
            assert!(
                !no_remaining_budget_set,
                "No remaining budget set, should return false"
            );

            set_campaign_remaining(&mut redis, campaign, 9_000).await;

            let not_enough_remaining_budget =
                has_enough_remaining_budget(&mut redis, campaign, amount)
                    .await
                    .expect("Should check campaign remaining");
            assert!(
                !not_enough_remaining_budget,
                "Not enough remaining budget, should return false"
            );

            set_campaign_remaining(&mut redis, campaign, 11_000).await;

            let has_enough_remaining_budget =
                has_enough_remaining_budget(&mut redis, campaign, amount)
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
            let amount = UnifiedNum::from(5_000);

            set_campaign_remaining(&mut redis, campaign, 9_000).await;

            let remaining = decrease_remaining_budget(&mut redis, campaign, amount)
                .await
                .expect("Should decrease campaign remaining");
            assert_eq!(
                4_000, remaining,
                "Should decrease remaining budget with amount and be positive"
            );

            let remaining = decrease_remaining_budget(&mut redis, campaign, amount)
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
                    redis.connection.clone(),
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
                    redis.connection.clone(),
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
                    10_640_i64,
                    get_campaign_remaining(&mut redis.connection, campaign.id)
                        .await
                        .expect("Should have key")
                )
            }
        }
    }
}
