use crate::db::{
    event_aggregate::{
        latest_approve_state, latest_approve_state_v5, latest_heartbeats, latest_new_state,
        latest_new_state_v5,
    },
    get_channel_by_id, insert_channel, insert_validator_messages, list_channels,
    spendable::{fetch_spendable, update_spendable},
    DbPool, PoolError,
};
use crate::{success_response, Application, Auth, ResponseError, RouteParams};
use futures::future::try_join_all;
use hex::FromHex;
use hyper::{Body, Request, Response};
use primitives::{
    adapter::Adapter,
    balances::UncheckedState,
    channel_v5::Channel as ChannelV5,
    config::TokenInfo,
    sentry::{
        channel_list::{ChannelListQuery, LastApprovedQuery},
        AllSpendersResponse, LastApproved, LastApprovedResponse, MessageResponse, SpenderResponse,
        SuccessResponse,
    },
    spender::{Deposit, Spendable, Spender, SpenderLeaf},
    validator::{MessageTypes, NewState},
    Address, Channel, ChannelId, UnifiedNum,
};
use slog::error;
use std::{collections::HashMap, str::FromStr};
use tokio_postgres::error::SqlState;

pub async fn channel_status<A: Adapter>(
    req: Request<Body>,
    _: &Application<A>,
) -> Result<Response<Body>, ResponseError> {
    use serde::Serialize;
    #[derive(Serialize)]
    struct ChannelStatusResponse<'a> {
        channel: &'a Channel,
    }

    let channel = req
        .extensions()
        .get::<Channel>()
        .expect("Request should have Channel");

    let response = ChannelStatusResponse { channel };

    Ok(success_response(serde_json::to_string(&response)?))
}

pub async fn create_channel<A: Adapter>(
    req: Request<Body>,
    app: &Application<A>,
) -> Result<Response<Body>, ResponseError> {
    let body = hyper::body::to_bytes(req.into_body()).await?;

    let channel = serde_json::from_slice::<Channel>(&body)
        .map_err(|e| ResponseError::FailedValidation(e.to_string()))?;

    // TODO AIP#61: No longer needed, remove!
    // if let Err(e) = app.adapter.validate_channel(&channel).await {
    //     return Err(ResponseError::BadRequest(e.to_string()));
    // }

    let error_response = ResponseError::BadRequest("err occurred; please try again later".into());

    match insert_channel(&app.pool, &channel).await {
        Err(error) => {
            error!(&app.logger, "{}", &error; "module" => "create_channel");

            match error {
                PoolError::Backend(error) if error.code() == Some(&SqlState::UNIQUE_VIOLATION) => {
                    Err(ResponseError::Conflict(
                        "channel already exists".to_string(),
                    ))
                }
                _ => Err(error_response),
            }
        }
        Ok(false) => Err(error_response),
        _ => Ok(()),
    }?;

    let create_response = SuccessResponse { success: true };

    Ok(success_response(serde_json::to_string(&create_response)?))
}

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
        &query.creator,
        &query.validator,
        &query.valid_until_ge,
    )
    .await?;

    Ok(success_response(serde_json::to_string(&list_response)?))
}

pub async fn channel_validate<A: Adapter>(
    req: Request<Body>,
    _: &Application<A>,
) -> Result<Response<Body>, ResponseError> {
    let body = hyper::body::to_bytes(req.into_body()).await?;
    let _channel = serde_json::from_slice::<Channel>(&body)
        .map_err(|e| ResponseError::FailedValidation(e.to_string()))?;
    let create_response = SuccessResponse { success: true };
    Ok(success_response(serde_json::to_string(&create_response)?))
}

pub async fn last_approved<A: Adapter>(
    req: Request<Body>,
    app: &Application<A>,
) -> Result<Response<Body>, ResponseError> {
    // get request params
    let route_params = req
        .extensions()
        .get::<RouteParams>()
        .expect("request should have route params");

    let channel_id = ChannelId::from_hex(route_params.index(0))?;
    let channel = get_channel_by_id(&app.pool, &channel_id).await?.unwrap();

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

    let approve_state = match latest_approve_state(&app.pool, &channel).await? {
        Some(approve_state) => approve_state,
        None => return Ok(default_response),
    };

    let state_root = approve_state.msg.state_root.clone();

    let new_state = latest_new_state(&app.pool, &channel, &state_root).await?;
    if new_state.is_none() {
        return Ok(default_response);
    }

    let query = serde_urlencoded::from_str::<LastApprovedQuery>(req.uri().query().unwrap_or(""))?;
    let validators = channel.spec.validators;
    let channel_id = channel.id;
    let heartbeats = if query.with_heartbeat.is_some() {
        let result = try_join_all(
            validators
                .iter()
                .map(|validator| latest_heartbeats(&app.pool, &channel_id, &validator.id)),
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

    match channel.spec.validators.find(&session.uid) {
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

async fn create_or_update_spendable_document(
    adapter: &impl Adapter,
    token_info: &TokenInfo,
    pool: DbPool,
    channel: &ChannelV5,
    spender: Address,
) -> Result<Spendable, ResponseError> {
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
        channel: channel.clone(),
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
        .get::<ChannelV5>()
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

    let new_state = match get_corresponding_new_state(&app.pool, &channel).await {
        Some(new_state) => new_state,
        None => return spender_response_without_leaf(latest_spendable.deposit.total),
    };

    let new_state_checked = new_state.msg.into_inner().try_checked()?;

    let total_spent = new_state_checked.balances.spenders.get(&spender);

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
        .get::<ChannelV5>()
        .expect("Request should have Channel")
        .to_owned();

    let new_state = get_corresponding_new_state(&app.pool, &channel)
        .await
        .ok_or(ResponseError::NotFound)?;

    let mut all_spender_limits: HashMap<Address, Spender> = HashMap::new();

    // Using for loop to avoid async closures
    for (spender_addr, balance) in new_state.msg.balances.spenders.iter() {
        let latest_spendable =
            match fetch_spendable(app.pool.clone(), spender_addr, &channel.id()).await? {
                Some(spendable) => spendable,
                None => return Err(ResponseError::NotFound),
            };

        let total_deposited = latest_spendable.deposit.total;
        let spender_leaf = get_spender_leaf_for_spender(balance, &total_deposited);

        let spender_info = Spender {
            total_deposited,
            spender_leaf,
        };

        all_spender_limits.insert(*spender_addr, spender_info);
    }

    let res = AllSpendersResponse {
        spenders: all_spender_limits,
    };

    Ok(success_response(serde_json::to_string(&res)?))
}

async fn get_corresponding_new_state(
    pool: &DbPool,
    channel: &ChannelV5,
) -> Option<MessageResponse<NewState<UncheckedState>>> {
    let approve_state = match latest_approve_state_v5(pool, channel).await.ok()? {
        Some(approve_state) => approve_state,
        None => return None,
    };

    let state_root = approve_state.msg.state_root.clone();

    let new_state = latest_new_state_v5(pool, channel, &state_root).await.ok()?;

    new_state
}

fn get_spender_leaf_for_spender(
    spender_balance: &UnifiedNum,
    total_deposited: &UnifiedNum,
) -> Option<SpenderLeaf> {
    let total_spent = match total_deposited.checked_sub(spender_balance) {
        Some(spent) => spent,
        None => return None,
    };

    // Return
    let leaf = SpenderLeaf {
        total_spent,
        // merkle_proof: [u8; 32], // TODO
    };

    Some(leaf)
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::test_util::setup_dummy_app;
    use primitives::{
        adapter::Deposit,
        util::tests::prep_db::{ADDRESSES, DUMMY_CAMPAIGN},
        BigNum,
    };

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
}
