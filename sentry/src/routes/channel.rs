use crate::db::{
    accounting::{get_all_accountings_for_channel, Side},
    event_aggregate::{latest_approve_state_v5, latest_heartbeats, latest_new_state_v5},
    insert_channel, insert_validator_messages, list_channels,
    spendable::{fetch_spendable, get_all_spendables_for_channel, update_spendable},
    DbPool,
};
use crate::{success_response, Application, Auth, ResponseError, RouteParams};
use futures::future::try_join_all;
use hyper::{Body, Request, Response};
use primitives::{
    adapter::Adapter,
    balances::{Balances, CheckedState, UncheckedState},
    config::TokenInfo,
    sentry::{
        channel_list::ChannelListQuery, AccountingResponse, AllSpendersResponse, LastApproved,
        LastApprovedQuery, LastApprovedResponse, Pagination, SpenderResponse, SuccessResponse,
    },
    spender::{Spendable, Spender, SpenderLeaf},
    validator::{MessageTypes, NewState},
    Address, Channel, Deposit, UnifiedNum,
};
use slog::{error, Logger};
use std::{collections::HashMap, str::FromStr};

pub async fn channel_list<A: Adapter>(
    req: Request<Body>,
    app: &Application<A>,
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

pub async fn last_approved<A: Adapter>(
    req: Request<Body>,
    app: &Application<A>,
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

pub async fn create_validator_messages<A: Adapter + 'static>(
    req: Request<Body>,
    app: &Application<A>,
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
async fn create_or_update_spendable_document(
    adapter: &impl Adapter,
    token_info: &TokenInfo,
    pool: DbPool,
    channel: &Channel,
    spender: Address,
) -> Result<Spendable, ResponseError> {
    insert_channel(&pool, *channel).await?;

    let deposit = adapter.get_deposit(channel, &spender).await?;
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
            spender_leaf: None,
        },
    };
    Ok(success_response(serde_json::to_string(&res)?))
}

pub async fn get_spender_limits<A: Adapter + 'static>(
    req: Request<Body>,
    app: &Application<A>,
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

    let total_spent = new_state.balances.spenders.get(&spender);

    let spender_leaf = total_spent.map(|total_spent| SpenderLeaf {
        total_spent: *total_spent,
        //merkle_proof: [u8; 32], // TODO
    });

    // returned output
    let res = SpenderResponse {
        spender: Spender {
            total_deposited: latest_spendable.deposit.total,
            spender_leaf,
        },
    };
    Ok(success_response(serde_json::to_string(&res)?))
}

pub async fn get_all_spender_limits<A: Adapter + 'static>(
    req: Request<Body>,
    app: &Application<A>,
) -> Result<Response<Body>, ResponseError> {
    let channel = req
        .extensions()
        .get::<Channel>()
        .expect("Request should have Channel")
        .to_owned();

    let new_state = get_corresponding_new_state(&app.pool, &app.logger, &channel).await?;

    let mut all_spender_limits: HashMap<Address, Spender> = HashMap::new();

    let all_spendables = get_all_spendables_for_channel(app.pool.clone(), &channel.id()).await?;

    // Using for loop to avoid async closures
    for spendable in all_spendables {
        let spender = spendable.spender;
        let spender_leaf = match new_state {
            Some(ref new_state) => new_state.balances.spenders.get(&spender).map(|balance| {
                SpenderLeaf {
                    total_spent: spendable
                        .deposit
                        .total
                        .checked_sub(balance)
                        .unwrap_or_default(),
                    // merkle_proof: [u8; 32], // TODO
                }
            }),
            None => None,
        };

        let spender_info = Spender {
            total_deposited: spendable.deposit.total,
            spender_leaf,
        };

        all_spender_limits.insert(spender, spender_info);
    }

    let res = AllSpendersResponse {
        spenders: all_spender_limits,
        pagination: Pagination {
            // TODO
            page: 1,
            total: 1,
            total_pages: 1,
        },
    };

    Ok(success_response(serde_json::to_string(&res)?))
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

pub async fn get_accounting_for_channel<A: Adapter + 'static>(
    req: Request<Body>,
    app: &Application<A>,
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
    use crate::test_util::setup_dummy_app;
    use crate::db::{accounting::update_accounting, insert_channel};
    use primitives::{
        adapter::Deposit,
        util::tests::prep_db::{ADDRESSES, DUMMY_CAMPAIGN},
        BigNum,
    };
    use hyper::StatusCode;

    #[tokio::test]
    async fn create_and_fetch_spendable() {
        let app = setup_dummy_app().await;

        let channel = DUMMY_CAMPAIGN.channel.clone();

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

        app.adapter
            .add_deposit_call(channel.id(), ADDRESSES["creator"], updated_deposit.clone());

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

    #[tokio::test]
    async fn get_accountings_for_channel() {
        let app = setup_dummy_app().await;
        let channel = DUMMY_CAMPAIGN.channel.clone();
        insert_channel(&app.pool, channel).await.expect("should insert channel");
        let req = Request::builder().extension(channel).body(Body::empty()).expect("Should build Request");

        update_accounting(app.pool.clone(), channel.id(), ADDRESSES["publisher"], Side::Earner, UnifiedNum::from_u64(200)).await.expect("should insert accounting");
        update_accounting(app.pool.clone(), channel.id(), ADDRESSES["publisher2"], Side::Earner, UnifiedNum::from_u64(100)).await.expect("should insert accounting");
        update_accounting(app.pool.clone(), channel.id(), ADDRESSES["creator"], Side::Spender, UnifiedNum::from_u64(200)).await.expect("should insert accounting");
        update_accounting(app.pool.clone(), channel.id(), ADDRESSES["tester"], Side::Spender, UnifiedNum::from_u64(100)).await.expect("should insert accounting");

        let accounting_response = get_accounting_for_channel(req, &app).await.expect("should get response");
        assert_eq!(StatusCode::OK, accounting_response.status());
        let json = hyper::body::to_bytes(accounting_response.into_body())
            .await
            .expect("Should get json");

        let accounting_response: AccountingResponse<CheckedState> =
            serde_json::from_slice(&json).expect("Should get AccouuntingResponse");
        let sum = accounting_response.balances.sum().expect("shouldn't be None");
        assert_eq!(sum.0, UnifiedNum::from_u64(300));
        assert_eq!(sum.1, UnifiedNum::from_u64(300));

        update_accounting(app.pool.clone(), channel.id(), ADDRESSES["tester"], Side::Spender, UnifiedNum::from_u64(200)).await.expect("should insert accounting");

        let new_req = Request::builder().extension(channel).body(Body::empty()).expect("Should build Request");
        let bad_accounting_response = get_accounting_for_channel(new_req, &app).await;
        assert!(bad_accounting_response.is_err())
    }
}
