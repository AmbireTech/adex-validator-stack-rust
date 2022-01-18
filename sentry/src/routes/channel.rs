use crate::db::{
    accounting::{get_all_accountings_for_channel, update_accounting, Side},
    event_aggregate::{latest_approve_state_v5, latest_heartbeats, latest_new_state_v5},
    insert_channel, insert_validator_messages, list_channels,
    spendable::{fetch_spendable, get_all_spendables_for_channel, update_spendable},
    DbPool,
};
use crate::{success_response, Application, Auth, ResponseError, RouteParams};
use adapter::{client::Locked, Adapter};
use futures::future::try_join_all;
use hyper::{Body, Request, Response};
use primitives::{
    balances::{Balances, CheckedState, UncheckedState},
    config::TokenInfo,
    sentry::{
        channel_list::ChannelListQuery, AccountingResponse, AllSpendersQuery, AllSpendersResponse,
        LastApproved, LastApprovedQuery, LastApprovedResponse, SpenderResponse, SuccessResponse,
    },
    spender::{Spendable, Spender},
    validator::{MessageTypes, NewState},
    Address, Channel, Deposit, UnifiedNum,
};
use slog::{error, Logger};
use std::{collections::HashMap, str::FromStr};

pub async fn channel_list<C: Locked + 'static>(
    req: Request<Body>,
    app: &Application<C>,
) -> Result<Response<Body>, ResponseError> {
    let query = serde_urlencoded::from_str::<ChannelListQuery>(req.uri().query().unwrap_or(""))?;
    let skip = query
        .page
        .checked_mul(app.config.channels_find_limit.into())
        .ok_or_else(|| ResponseError::BadRequest("Page and/or limit is too large".into()))?;

    let list_response = list_channels(
        &app.pool,
        skip,
        app.config.channels_find_limit,
        query.validator,
    )
    .await?;

    Ok(success_response(serde_json::to_string(&list_response)?))
}

pub async fn last_approved<C: Locked + 'static>(
    req: Request<Body>,
    app: &Application<C>,
) -> Result<Response<Body>, ResponseError> {
    // get request Channel
    let channel = *req
        .extensions()
        .get::<Channel>()
        .ok_or(ResponseError::NotFound)?;

    let default_response = Response::builder()
        .header("Content-type", "application/json")
        .body(
            serde_json::to_string(&LastApprovedResponse::<UncheckedState> {
                last_approved: None,
                heartbeats: None,
            })?
            .into(),
        )
        .expect("should build response");

    let approve_state = match latest_approve_state_v5(&app.pool, &channel).await? {
        Some(approve_state) => approve_state,
        None => return Ok(default_response),
    };

    let state_root = approve_state.msg.state_root.clone();

    let new_state = latest_new_state_v5(&app.pool, &channel, &state_root).await?;
    if new_state.is_none() {
        return Ok(default_response);
    }

    let query = serde_urlencoded::from_str::<LastApprovedQuery>(req.uri().query().unwrap_or(""))?;
    let validators = vec![channel.leader, channel.follower];
    let channel_id = channel.id();
    let heartbeats = if query.with_heartbeat.is_some() {
        let result = try_join_all(
            validators
                .iter()
                .map(|validator| latest_heartbeats(&app.pool, &channel_id, validator)),
        )
        .await?;
        Some(result.into_iter().flatten().collect::<Vec<_>>())
    } else {
        None
    };

    Ok(Response::builder()
        .header("Content-type", "application/json")
        .body(
            serde_json::to_string(&LastApprovedResponse {
                last_approved: Some(LastApproved {
                    new_state,
                    approve_state: Some(approve_state),
                }),
                heartbeats,
            })?
            .into(),
        )
        .unwrap())
}

pub async fn create_validator_messages<C: Locked + 'static>(
    req: Request<Body>,
    app: &Application<C>,
) -> Result<Response<Body>, ResponseError> {
    let session = req
        .extensions()
        .get::<Auth>()
        .expect("auth request session")
        .to_owned();

    let channel = req
        .extensions()
        .get::<Channel>()
        .expect("Request should have Channel")
        .to_owned();

    let into_body = req.into_body();
    let body = hyper::body::to_bytes(into_body).await?;

    let request_body = serde_json::from_slice::<HashMap<String, Vec<MessageTypes>>>(&body)?;
    let messages = request_body
        .get("messages")
        .ok_or_else(|| ResponseError::BadRequest("missing messages body".to_string()))?;

    match channel.find_validator(session.uid) {
        None => Err(ResponseError::Unauthorized),
        _ => {
            try_join_all(messages.iter().map(|message| {
                insert_validator_messages(&app.pool, &channel, &session.uid, message)
            }))
            .await?;

            Ok(success_response(serde_json::to_string(&SuccessResponse {
                success: true,
            })?))
        }
    }
}

/// This will make sure to insert/get the `Channel` from DB before attempting to create the `Spendable`
async fn create_or_update_spendable_document<A: Locked>(
    adapter: &Adapter<A>,
    token_info: &TokenInfo,
    pool: DbPool,
    channel: &Channel,
    spender: Address,
) -> Result<Spendable, ResponseError> {
    insert_channel(&pool, *channel).await?;

    let deposit = adapter.get_deposit(channel, spender).await?;
    let total = UnifiedNum::from_precision(deposit.total, token_info.precision.get());
    let still_on_create2 =
        UnifiedNum::from_precision(deposit.still_on_create2, token_info.precision.get());
    let (total, still_on_create2) = match (total, still_on_create2) {
        (Some(total), Some(still_on_create2)) => (total, still_on_create2),
        _ => {
            return Err(ResponseError::BadRequest(
                "couldn't get deposit from precision".to_string(),
            ))
        }
    };

    let spendable = Spendable {
        channel: *channel,
        deposit: Deposit {
            total,
            still_on_create2,
        },
        spender,
    };

    // Insert latest spendable in DB
    update_spendable(pool, &spendable).await?;

    Ok(spendable)
}

fn spender_response_without_leaf(
    total_deposited: UnifiedNum,
) -> Result<Response<Body>, ResponseError> {
    let res = SpenderResponse {
        spender: Spender {
            total_deposited,
            total_spent: None,
        },
    };
    Ok(success_response(serde_json::to_string(&res)?))
}

pub async fn get_spender_limits<C: Locked + 'static>(
    req: Request<Body>,
    app: &Application<C>,
) -> Result<Response<Body>, ResponseError> {
    let route_params = req
        .extensions()
        .get::<RouteParams>()
        .expect("request should have route params");

    let channel = req
        .extensions()
        .get::<Channel>()
        .expect("Request should have Channel")
        .to_owned();

    let spender = Address::from_str(&route_params.index(1))?;

    let latest_spendable = fetch_spendable(app.pool.clone(), &spender, &channel.id()).await?;

    let token_info = app
        .config
        .token_address_whitelist
        .get(&channel.token)
        .ok_or_else(|| ResponseError::FailedValidation("Unsupported Channel Token".to_string()))?;

    let latest_spendable = match latest_spendable {
        Some(spendable) => spendable,
        None => {
            create_or_update_spendable_document(
                &app.adapter,
                token_info,
                app.pool.clone(),
                &channel,
                spender,
            )
            .await?
        }
    };

    let new_state = match get_corresponding_new_state(&app.pool, &app.logger, &channel).await? {
        Some(new_state) => new_state,
        None => return spender_response_without_leaf(latest_spendable.deposit.total),
    };

    let total_spent = new_state
        .balances
        .spenders
        .get(&spender)
        .map(|spent| spent.to_owned());

    // returned output
    let res = SpenderResponse {
        spender: Spender {
            total_deposited: latest_spendable.deposit.total,
            total_spent,
        },
    };
    Ok(success_response(serde_json::to_string(&res)?))
}

pub async fn get_all_spender_limits<C: Locked + 'static>(
    req: Request<Body>,
    app: &Application<C>,
) -> Result<Response<Body>, ResponseError> {
    let channel = req
        .extensions()
        .get::<Channel>()
        .expect("Request should have Channel")
        .to_owned();

    let query = serde_urlencoded::from_str::<AllSpendersQuery>(req.uri().query().unwrap_or(""))?;
    let limit = app.config.spendable_find_limit;
    let skip = query
        .page
        .checked_mul(limit.into())
        .ok_or_else(|| ResponseError::FailedValidation("Page and/or limit is too large".into()))?;

    let new_state = get_corresponding_new_state(&app.pool, &app.logger, &channel).await?;

    let mut all_spender_limits: HashMap<Address, Spender> = HashMap::new();

    let (all_spendables, pagination) =
        get_all_spendables_for_channel(app.pool.clone(), &channel.id(), skip, limit.into()).await?;

    // Using for loop to avoid async closures
    for spendable in all_spendables {
        let spender = spendable.spender;
        let total_spent = match new_state {
            Some(ref new_state) => new_state.balances.spenders.get(&spender).map(|balance| {
                spendable
                    .deposit
                    .total
                    .checked_sub(balance)
                    .unwrap_or_default()
            }),
            None => None,
        };

        let spender_info = Spender {
            total_deposited: spendable.deposit.total,
            total_spent,
        };

        all_spender_limits.insert(spender, spender_info);
    }

    let res = AllSpendersResponse {
        spenders: all_spender_limits,
        pagination,
    };

    Ok(success_response(serde_json::to_string(&res)?))
}

/// internally, to make the validator worker to add a spender leaf in NewState we'll just update Accounting
pub async fn add_spender_leaf<C: Locked + 'static>(
    req: Request<Body>,
    app: &Application<C>,
) -> Result<Response<Body>, ResponseError> {
    let route_params = req
        .extensions()
        .get::<RouteParams>()
        .expect("request should have route params");
    let spender = Address::from_str(&route_params.index(1))?;

    let channel = req
        .extensions()
        .get::<Channel>()
        .expect("Request should have Channel")
        .to_owned();

    update_accounting(
        app.pool.clone(),
        channel.id(),
        spender,
        Side::Spender,
        UnifiedNum::from_u64(0),
    )
    .await?;

    // TODO: Replace with SpenderResponse
    Ok(success_response(serde_json::to_string(&SuccessResponse {
        success: true,
    })?))
}

async fn get_corresponding_new_state(
    pool: &DbPool,
    logger: &Logger,
    channel: &Channel,
) -> Result<Option<NewState<CheckedState>>, ResponseError> {
    let approve_state = match latest_approve_state_v5(pool, channel).await? {
        Some(approve_state) => approve_state,
        None => return Ok(None),
    };

    let state_root = approve_state.msg.state_root.clone();

    let new_state = match latest_new_state_v5(pool, channel, &state_root).await? {
        Some(new_state) => {
            let new_state = new_state.msg.into_inner().try_checked().map_err(|err| {
                error!(&logger, "Balances are not aligned in an approved NewState: {}", &err; "module" => "get_spender_limits");
                ResponseError::BadRequest("Balances are not aligned in an approved NewState".to_string())
            })?;
            Ok(Some(new_state))
        }
        None => {
            error!(&logger, "{}", "Fatal error! The NewState for the last ApproveState was not found"; "module" => "get_spender_limits");
            return Err(ResponseError::BadRequest(
                "Fatal error! The NewState for the last ApproveState was not found".to_string(),
            ));
        }
    };

    new_state
}

pub async fn get_accounting_for_channel<C: Locked + 'static>(
    req: Request<Body>,
    app: &Application<C>,
) -> Result<Response<Body>, ResponseError> {
    let channel = req
        .extensions()
        .get::<Channel>()
        .expect("Request should have Channel")
        .to_owned();

    let accountings = get_all_accountings_for_channel(app.pool.clone(), channel.id()).await?;

    let mut unchecked_balances: Balances<UncheckedState> = Balances::default();

    for accounting in accountings {
        match accounting.side {
            Side::Earner => unchecked_balances
                .earners
                .insert(accounting.address, accounting.amount),
            Side::Spender => unchecked_balances
                .spenders
                .insert(accounting.address, accounting.amount),
        };
    }

    let balances = match unchecked_balances.check() {
        Ok(balances) => balances,
        Err(error) => {
            error!(&app.logger, "{}", &error; "module" => "channel_accounting");
            return Err(ResponseError::FailedValidation(
                "Earners sum is not equal to spenders sum for channel".to_string(),
            ));
        }
    };

    let res = AccountingResponse::<CheckedState> { balances };
    Ok(success_response(serde_json::to_string(&res)?))
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::db::{accounting::spend_amount, insert_channel};
    use crate::test_util::setup_dummy_app;
    use adapter::primitives::Deposit;
    use hyper::StatusCode;
    use primitives::{
        test_util::{ADVERTISER, CREATOR, GUARDIAN, PUBLISHER},
        util::tests::prep_db::{ADDRESSES, DUMMY_CAMPAIGN, IDS},
        BigNum,
    };

    #[tokio::test]
    async fn create_and_fetch_spendable() {
        let app = setup_dummy_app().await;

        let channel = DUMMY_CAMPAIGN.channel;

        let token_info = app
            .config
            .token_address_whitelist
            .get(&channel.token)
            .expect("should retrieve address");
        let precision: u8 = token_info.precision.into();
        let deposit = Deposit {
            total: BigNum::from_str("100000000000000000000").expect("should convert"), // 100 DAI
            still_on_create2: BigNum::from_str("1000000000000000000").expect("should convert"), // 1 DAI
        };
        app.adapter
            .client
            .add_deposit_call(channel.id(), ADDRESSES["creator"], deposit.clone());
        // Making sure spendable does not yet exist
        let spendable = fetch_spendable(app.pool.clone(), &ADDRESSES["creator"], &channel.id())
            .await
            .expect("should return None");
        assert!(spendable.is_none());
        // Call create_or_update_spendable
        let new_spendable = create_or_update_spendable_document(
            &app.adapter,
            token_info,
            app.pool.clone(),
            &channel,
            ADDRESSES["creator"],
        )
        .await
        .expect("should create a new spendable");
        assert_eq!(new_spendable.channel.id(), channel.id());

        let total_as_unified_num =
            UnifiedNum::from_precision(deposit.total, precision).expect("should convert");
        let still_on_create2_unified =
            UnifiedNum::from_precision(deposit.still_on_create2, precision)
                .expect("should convert");
        assert_eq!(new_spendable.deposit.total, total_as_unified_num);
        assert_eq!(
            new_spendable.deposit.still_on_create2,
            still_on_create2_unified
        );
        assert_eq!(new_spendable.spender, ADDRESSES["creator"]);

        // Make sure spendable NOW exists
        let spendable = fetch_spendable(app.pool.clone(), &ADDRESSES["creator"], &channel.id())
            .await
            .expect("should return a spendable");
        assert!(spendable.is_some());

        let updated_deposit = Deposit {
            total: BigNum::from_str("110000000000000000000").expect("should convert"), // 110 DAI
            still_on_create2: BigNum::from_str("1100000000000000000").expect("should convert"), // 1.1 DAI
        };

        app.adapter.client.add_deposit_call(
            channel.id(),
            ADDRESSES["creator"],
            updated_deposit.clone(),
        );

        let updated_spendable = create_or_update_spendable_document(
            &app.adapter,
            token_info,
            app.pool.clone(),
            &channel,
            ADDRESSES["creator"],
        )
        .await
        .expect("should update spendable");
        let total_as_unified_num =
            UnifiedNum::from_precision(updated_deposit.total, precision).expect("should convert");
        let still_on_create2_unified =
            UnifiedNum::from_precision(updated_deposit.still_on_create2, precision)
                .expect("should convert");
        assert_eq!(updated_spendable.deposit.total, total_as_unified_num);
        assert_eq!(
            updated_spendable.deposit.still_on_create2,
            still_on_create2_unified
        );
        assert_eq!(updated_spendable.spender, ADDRESSES["creator"]);
    }

    async fn res_to_accounting_response(res: Response<Body>) -> AccountingResponse<CheckedState> {
        let json = hyper::body::to_bytes(res.into_body())
            .await
            .expect("Should get json");

        let accounting_response: AccountingResponse<CheckedState> =
            serde_json::from_slice(&json).expect("Should get AccouuntingResponse");
        accounting_response
    }

    #[tokio::test]
    async fn get_accountings_for_channel() {
        let app = setup_dummy_app().await;
        let channel = DUMMY_CAMPAIGN.channel;
        insert_channel(&app.pool, channel)
            .await
            .expect("should insert channel");
        let build_request = |channel: Channel| {
            Request::builder()
                .extension(channel)
                .body(Body::empty())
                .expect("Should build Request")
        };
        // Testing for no accounting yet
        {
            let res = get_accounting_for_channel(build_request(channel), &app)
                .await
                .expect("should get response");
            assert_eq!(StatusCode::OK, res.status());

            let accounting_response = res_to_accounting_response(res).await;
            assert_eq!(accounting_response.balances.earners.len(), 0);
            assert_eq!(accounting_response.balances.spenders.len(), 0);
        }

        // Testing for 2 accountings - first channel
        {
            let mut balances = Balances::<CheckedState>::new();
            balances
                .spend(
                    ADDRESSES["creator"],
                    ADDRESSES["publisher"],
                    UnifiedNum::from_u64(200),
                )
                .expect("should not overflow");
            balances
                .spend(
                    ADDRESSES["tester"],
                    ADDRESSES["publisher2"],
                    UnifiedNum::from_u64(100),
                )
                .expect("Should not overflow");
            spend_amount(app.pool.clone(), channel.id(), balances.clone())
                .await
                .expect("should spend");

            let res = get_accounting_for_channel(build_request(channel), &app)
                .await
                .expect("should get response");
            assert_eq!(StatusCode::OK, res.status());

            let accounting_response = res_to_accounting_response(res).await;

            assert_eq!(balances, accounting_response.balances);
        }

        // Testing for 2 accountings - second channel (same address is both an earner and a spender)
        {
            let mut second_channel = DUMMY_CAMPAIGN.channel;
            second_channel.leader = IDS["user"]; // channel.id() will be different now
            insert_channel(&app.pool, second_channel)
                .await
                .expect("should insert channel");

            let mut balances = Balances::<CheckedState>::new();
            balances
                .spend(ADDRESSES["tester"], ADDRESSES["publisher"], 300.into())
                .expect("Should not overflow");

            balances
                .spend(ADDRESSES["publisher"], ADDRESSES["user"], 300.into())
                .expect("Should not overflow");

            spend_amount(app.pool.clone(), second_channel.id(), balances.clone())
                .await
                .expect("should spend");

            let res = get_accounting_for_channel(build_request(second_channel), &app)
                .await
                .expect("should get response");
            assert_eq!(StatusCode::OK, res.status());

            let accounting_response = res_to_accounting_response(res).await;

            assert_eq!(balances, accounting_response.balances)
        }

        // Testing for when sums don't match on first channel - Error case
        {
            let mut balances = Balances::<CheckedState>::new();
            balances
                .earners
                .insert(ADDRESSES["publisher"], UnifiedNum::from_u64(100));
            balances
                .spenders
                .insert(ADDRESSES["creator"], UnifiedNum::from_u64(200));
            spend_amount(app.pool.clone(), channel.id(), balances)
                .await
                .expect("should spend");

            let res = get_accounting_for_channel(build_request(channel), &app).await;
            let expected = ResponseError::FailedValidation(
                "Earners sum is not equal to spenders sum for channel".to_string(),
            );
            assert_eq!(expected, res.expect_err("Should return an error"));
        }
    }

    #[tokio::test]
    async fn adds_and_retrieves_spender_leaf() {
        let app = setup_dummy_app().await;
        let channel = DUMMY_CAMPAIGN.channel;

        insert_channel(&app.pool, channel)
            .await
            .expect("should insert channel");

        let get_accounting_request = |channel: Channel| {
            Request::builder()
                .extension(channel)
                .body(Body::empty())
                .expect("Should build Request")
        };
        let add_spender_request = |channel: Channel| {
            let param = RouteParams(vec![channel.id().to_string(), CREATOR.to_string()]);
            Request::builder()
                .extension(channel)
                .extension(param)
                .body(Body::empty())
                .expect("Should build Request")
        };

        // Calling with non existent accounting
        let res = add_spender_leaf(add_spender_request(channel), &app)
            .await
            .expect("Should add");
        assert_eq!(StatusCode::OK, res.status());

        let res = get_accounting_for_channel(get_accounting_request(channel), &app)
            .await
            .expect("should get response");
        assert_eq!(StatusCode::OK, res.status());

        let accounting_response = res_to_accounting_response(res).await;

        // Making sure a new entry has been created
        assert_eq!(
            accounting_response.balances.spenders.get(&CREATOR),
            Some(&UnifiedNum::from_u64(0)),
        );

        let mut balances = Balances::<CheckedState>::new();
        balances
            .spend(*CREATOR, *PUBLISHER, UnifiedNum::from_u64(200))
            .expect("should not overflow");
        balances
            .spend(*ADVERTISER, *GUARDIAN, UnifiedNum::from_u64(100))
            .expect("Should not overflow");
        spend_amount(app.pool.clone(), channel.id(), balances.clone())
            .await
            .expect("should spend");

        let res = get_accounting_for_channel(get_accounting_request(channel), &app)
            .await
            .expect("should get response");
        assert_eq!(StatusCode::OK, res.status());

        let accounting_response = res_to_accounting_response(res).await;

        assert_eq!(balances, accounting_response.balances);

        let res = add_spender_leaf(add_spender_request(channel), &app)
            .await
            .expect("Should add");
        assert_eq!(StatusCode::OK, res.status());

        let res = get_accounting_for_channel(get_accounting_request(channel), &app)
            .await
            .expect("should get response");
        assert_eq!(StatusCode::OK, res.status());

        let accounting_response = res_to_accounting_response(res).await;

        // Balances shouldn't change
        assert_eq!(
            accounting_response.balances.spenders.get(&CREATOR),
            balances.spenders.get(&CREATOR),
        );
    }
}
