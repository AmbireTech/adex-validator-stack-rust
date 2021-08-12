use crate::db::{
    event_aggregate::{latest_approve_state, latest_heartbeats, latest_new_state, latest_new_state_v5, latest_approve_state_v5},
    spendable::{fetch_spendable, update_spendable},
    get_channel_by_id, insert_channel, insert_validator_messages, list_channels,
    update_exhausted_channel, PoolError
};
use crate::{success_response, Application, Auth, ResponseError, RouteParams};
use futures::future::try_join_all;
use hex::FromHex;
use hyper::{Body, Request, Response};
use primitives::{
    adapter::Adapter,
    sentry::{
        channel_list::{ChannelListQuery, LastApprovedQuery},
        LastApproved, LastApprovedResponse, SuccessResponse, SpenderLeaf, SpenderResponse
    },
    spender::{Spendable, Deposit},
    validator::MessageTypes,
    channel_v5::Channel as ChannelV5,
    Address, BigNum, Channel, ChannelId, UnifiedNum
};
use slog::error;
use std::{
    collections::HashMap,
    str::FromStr,
};
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
            serde_json::to_string(&LastApprovedResponse {
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

    let channel_is_exhausted = messages.iter().any(|message| match message {
        MessageTypes::ApproveState(approve) => approve.exhausted,
        MessageTypes::NewState(new_state) => new_state.exhausted,
        _ => false,
    });

    match channel.spec.validators.find(&session.uid) {
        None => Err(ResponseError::Unauthorized),
        _ => {
            try_join_all(messages.iter().map(|message| {
                insert_validator_messages(&app.pool, &channel, &session.uid, message)
            }))
            .await?;

            if channel_is_exhausted {
                if let Some(validator_index) = channel.spec.validators.find_index(&session.uid) {
                    update_exhausted_channel(&app.pool, &channel, validator_index).await?;
                }
            }

            Ok(success_response(serde_json::to_string(&SuccessResponse {
                success: true,
            })?))
        }
    }
}

async fn create_spendable_document<A: Adapter + 'static>(app: &Application<A>, channel: &ChannelV5, spender: &Address) -> Result<Spendable, ResponseError> {
    let deposit = app.adapter.get_deposit(&channel, &spender).await?;
    let token_info = app.config.token_address_whitelist.get(&channel.token).ok_or_else(|| ResponseError::BadRequest("channel has invalid token".to_string()))?; // I don't think this error can happen
    let divisor = 10u64.pow(token_info.precision.get().into());
    let total = UnifiedNum::from_u64(deposit.total.div_floor(&BigNum::from(divisor)).to_u64().expect("should convert"));
    let still_on_create2 = UnifiedNum::from_u64(deposit.still_on_create2.div_floor(&BigNum::from(divisor)).to_u64().expect("should convert"));

    let spendable = Spendable {
        channel: channel.clone(),
        deposit: Deposit {
            total,
            still_on_create2,
        },
        spender: spender.clone(),
    };

    // Insert latest spendable in DB
    update_spendable(app.pool.clone(), &spendable).await?;

    Ok(spendable)
}

fn spender_response_without_leaf(total: UnifiedNum) -> Result<Response<Body>, ResponseError> {
    let res = SpenderResponse {
        total,
        spender_leaf: None,
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

    let channel_id = channel.id();
    let spender = Address::from_str(&route_params.index(1))?;

    let latest_spendable = fetch_spendable(app.pool.clone(), &spender, &channel_id)
        .await?;
    let latest_spendable = match latest_spendable {
        Some(spendable) => spendable,
        None => create_spendable_document(&app, &channel, &spender).await?,
    };

    let total = latest_spendable.deposit.total.checked_add(&latest_spendable.deposit.still_on_create2).ok_or_else(|| ResponseError::BadRequest("Total Deposited is too large".to_string()))?;
    let approve_state = match latest_approve_state_v5(&app.pool, &channel).await? {
        Some(approve_state) => approve_state,
        None => return spender_response_without_leaf(total),
    };

    let state_root = approve_state.msg.state_root.clone();

    let new_state = match latest_new_state_v5(&app.pool, &channel, &state_root).await? {
        Some(new_state) => new_state,
        None => return spender_response_without_leaf(total),
    };

    // TODO: Temporary until NewState uses Balances<CheckedState>
    let token_info = app.config.token_address_whitelist.get(&channel.token).ok_or_else(|| ResponseError::BadRequest("channel has invalid token".to_string()))?;
    let divisor = 10u64.pow(token_info.precision.get().into());

    let total_spent = match new_state.msg.balances.get(&spender) {
        Some(amount) => UnifiedNum::from_u64(amount.div_floor(&BigNum::from(divisor)).to_u64().expect("should convert")),
        None => UnifiedNum::from_u64(0),
    };

    let spender_leaf = Some(SpenderLeaf {
        total_spent,
        //merkle_proof: [u8; 32], // TODO
    });

    // returned output
    let res = SpenderResponse {
        total,
        spender_leaf,
    };
    Ok(success_response(serde_json::to_string(&res)?))
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        db::tests_postgres::{DATABASE_POOL, setup_test_migrations},
    };
    use adapter::DummyAdapter;
    use primitives::adapter::{DummyAdapterOptions, Deposit};
    use primitives::util::tests::prep_db::{AUTH, ADDRESSES, DUMMY_CAMPAIGN, IDS};
    use primitives::config::configuration;
    use primitives::BigNum;

    #[tokio::test]
    async fn create_and_fetch_spendable() {
        let database = DATABASE_POOL.get().await.expect("Should get a DB pool");
        let adapter_options = DummyAdapterOptions {
            dummy_identity: IDS["leader"],
            dummy_auth: IDS.clone(),
            dummy_auth_tokens: AUTH.clone(),
        };
        let config = configuration("development", None).expect("Dev config should be available");
        let dummy_adapter = DummyAdapter::init(adapter_options, &config);
        setup_test_migrations(database.pool.clone())
        .await
        .expect("Migrations should succeed");
        let channel = DUMMY_CAMPAIGN.channel.clone();
        let deposit = Deposit {
            total: BigNum::from(1000000000),
            still_on_create2: BigNum::from(1000000),
        };
        dummy_adapter.add_deposit_call(channel.id(), ADDRESSES["creator"], deposit.clone());

        // Making sure spendable does not yet exist
        let spendable = fetch_spendable(database.pool.clone(), &ADDRESSES["creator"], &channel.id()).await.expect("should return None");
        assert!(spendable.is_none());

        // Call create_spendable
        let new_spendable = create_spendable_document(&dummy_adapter, database.clone(), &channel, &ADDRESSES["creator"]).await.expect("should create a new spendable");
        assert_eq!(new_spendable.channel.id(), channel.id());
        assert_eq!(new_spendable.deposit.total, UnifiedNum::from_u64(deposit.total.to_u64().expect("should convert")));
        assert_eq!(new_spendable.deposit.still_on_create2, UnifiedNum::from_u64(deposit.still_on_create2.to_u64().expect("should convert")));
        assert_eq!(new_spendable.spender, ADDRESSES["creator"]);

        // Make sure spendable NOW exists
        let spendable = fetch_spendable(database.pool.clone(), &ADDRESSES["creator"], &channel.id()).await.expect("should return a spendable");
        assert!(spendable.is_some());
    }
}