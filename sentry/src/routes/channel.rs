//! Channel - `/v5/channel` routes
//!

use crate::db::{
    accounting::{get_all_accountings_for_channel, update_accounting, Side},
    event_aggregate::{latest_approve_state_v5, latest_heartbeats, latest_new_state_v5},
    insert_channel, list_channels,
    spendable::{fetch_spendable, get_all_spendables_for_channel, update_spendable},
    DbPool,
};
use crate::{success_response, Application, ResponseError, RouteParams};
use adapter::{client::Locked, Adapter};
use futures::future::try_join_all;
use hyper::{Body, Request, Response};
use primitives::{
    balances::{Balances, CheckedState, UncheckedState},
    sentry::{
        channel_list::ChannelListQuery, AccountingResponse, AllSpendersQuery, AllSpendersResponse,
        LastApproved, LastApprovedQuery, LastApprovedResponse, SpenderResponse, SuccessResponse,
    },
    spender::{Spendable, Spender},
    validator::NewState,
    Address, ChainOf, Channel, Deposit, UnifiedNum,
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
    let channel = req
        .extensions()
        .get::<ChainOf<Channel>>()
        .ok_or(ResponseError::NotFound)?
        .context;

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

/// This will make sure to insert/get the `Channel` from DB before attempting to create the `Spendable`
async fn create_or_update_spendable_document<A: Locked>(
    adapter: &Adapter<A>,
    pool: DbPool,
    channel_context: &ChainOf<Channel>,
    spender: Address,
) -> Result<Spendable, ResponseError> {
    insert_channel(&pool, channel_context.context).await?;

    let deposit = adapter.get_deposit(channel_context, spender).await?;
    let total = UnifiedNum::from_precision(deposit.total, channel_context.token.precision.get());
    let still_on_create2 = UnifiedNum::from_precision(
        deposit.still_on_create2,
        channel_context.token.precision.get(),
    );
    let (total, still_on_create2) = match (total, still_on_create2) {
        (Some(total), Some(still_on_create2)) => (total, still_on_create2),
        _ => {
            return Err(ResponseError::BadRequest(
                "couldn't get deposit from precision".to_string(),
            ))
        }
    };

    let spendable = Spendable {
        channel: channel_context.context,
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

    let channel_context = req
        .extensions()
        .get::<ChainOf<Channel>>()
        .expect("Request should have Channel & Chain/TokenInfo")
        .to_owned();
    let channel = &channel_context.context;

    let spender = Address::from_str(&route_params.index(1))?;

    let latest_spendable = fetch_spendable(app.pool.clone(), &spender, &channel.id()).await?;

    let latest_spendable = match latest_spendable {
        Some(spendable) => spendable,
        None => {
            create_or_update_spendable_document(
                &app.adapter,
                app.pool.clone(),
                &channel_context,
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
        .get::<ChainOf<Channel>>()
        .expect("Request should have Channel")
        .context;

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
        .get::<ChainOf<Channel>>()
        .ok_or(ResponseError::NotFound)?
        .context;

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

/// `GET /v5/channel/0xXXX.../accounting` request
///
/// Response: [`AccountingResponse::<CheckedState>`]
pub async fn get_accounting_for_channel<C: Locked + 'static>(
    req: Request<Body>,
    app: &Application<C>,
) -> Result<Response<Body>, ResponseError> {
    let channel = req
        .extensions()
        .get::<ChainOf<Channel>>()
        .ok_or(ResponseError::NotFound)?
        .context;

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

/// [`Channel`] [validator messages](primitives::validator::MessageTypes) routes
/// starting with `/v5/channel/0xXXX.../validator-messages`
///
pub mod validator_message {
    use std::collections::HashMap;

    use crate::{
        db::{get_validator_messages, insert_validator_messages},
        Auth,
    };
    use crate::{success_response, Application, ResponseError};
    use adapter::client::Locked;
    use futures::future::try_join_all;
    use hyper::{Body, Request, Response};
    use primitives::{
        sentry::{SuccessResponse, ValidatorMessageResponse},
        validator::MessageTypes,
        ChainOf,
    };
    use primitives::{Channel, DomainError, ValidatorId};
    use serde::Deserialize;

    #[derive(Deserialize)]
    pub struct ValidatorMessagesListQuery {
        limit: Option<u64>,
    }

    pub fn extract_params(
        from_path: &str,
    ) -> Result<(Option<ValidatorId>, Vec<String>), DomainError> {
        // trim the `/` at the beginning & end if there is one or more
        // and split the rest of the string at the `/`
        let split: Vec<&str> = from_path.trim_matches('/').split('/').collect();

        if split.len() > 2 {
            return Err(DomainError::InvalidArgument(
                "Too many parameters".to_string(),
            ));
        }

        let validator_id = split
            .get(0)
            // filter an empty string
            .filter(|string| !string.is_empty())
            // then try to map it to ValidatorId
            .map(|string| string.parse())
            // Transpose in order to check for an error from the conversion
            .transpose()?;

        let message_types = split
            .get(1)
            .filter(|string| !string.is_empty())
            .map(|string| string.split('+').map(|s| s.to_string()).collect());

        Ok((validator_id, message_types.unwrap_or_default()))
    }

    /// `GET /v5/channel/0xXXX.../validator-messages`
    /// with query parameters: [`ValidatorMessagesListQuery`].
    pub async fn list_validator_messages<C: Locked + 'static>(
        req: Request<Body>,
        app: &Application<C>,
        validator_id: &Option<ValidatorId>,
        message_types: &[String],
    ) -> Result<Response<Body>, ResponseError> {
        let query = serde_urlencoded::from_str::<ValidatorMessagesListQuery>(
            req.uri().query().unwrap_or(""),
        )?;

        let channel = req
            .extensions()
            .get::<ChainOf<Channel>>()
            .ok_or(ResponseError::NotFound)?
            .context;

        let config_limit = app.config.msgs_find_limit as u64;
        let limit = query
            .limit
            .filter(|n| *n >= 1)
            .unwrap_or(config_limit)
            .min(config_limit);

        let validator_messages =
            get_validator_messages(&app.pool, &channel.id(), validator_id, message_types, limit)
                .await?;

        let response = ValidatorMessageResponse { validator_messages };

        Ok(success_response(serde_json::to_string(&response)?))
    }

    /// `POST /v5/channel/0xXXX.../validator-messages` with Request body (json):
    /// ```json
    /// {
    ///     "messages": [
    ///         /// validator messages
    ///         ...
    ///     ]
    /// }
    /// ```
    ///
    /// Validator messages: [`MessageTypes`]
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
            .get::<ChainOf<Channel>>()
            .ok_or(ResponseError::NotFound)?
            .context;

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

        let (channel_context, channel) = {
            let channel = DUMMY_CAMPAIGN.channel;
            let channel_context = app
                .config
                .find_chain_token(DUMMY_CAMPAIGN.channel.token)
                .expect("should retrieve Chain & token");

            (channel_context.with_channel(channel), channel)
        };

        let precision: u8 = channel_context.token.precision.into();
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
            app.pool.clone(),
            &channel_context,
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
            app.pool.clone(),
            &channel_context,
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
        let channel_context = app
            .config
            .find_chain_token(DUMMY_CAMPAIGN.channel.token)
            .expect("Dummy channel Token should be present in config!")
            .with(DUMMY_CAMPAIGN.channel);

        insert_channel(&app.pool, channel_context.context)
            .await
            .expect("should insert channel");
        let build_request = |channel_context: &ChainOf<Channel>| {
            Request::builder()
                .extension(channel_context.clone())
                .body(Body::empty())
                .expect("Should build Request")
        };
        // Testing for no accounting yet
        {
            let res = get_accounting_for_channel(build_request(&channel_context), &app)
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
            spend_amount(
                app.pool.clone(),
                channel_context.context.id(),
                balances.clone(),
            )
            .await
            .expect("should spend");

            let res = get_accounting_for_channel(build_request(&channel_context), &app)
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

            let res = get_accounting_for_channel(
                build_request(&channel_context.clone().with(second_channel)),
                &app,
            )
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
            spend_amount(app.pool.clone(), channel_context.context.id(), balances)
                .await
                .expect("should spend");

            let res = get_accounting_for_channel(build_request(&channel_context), &app).await;
            let expected = ResponseError::FailedValidation(
                "Earners sum is not equal to spenders sum for channel".to_string(),
            );
            assert_eq!(expected, res.expect_err("Should return an error"));
        }
    }

    #[tokio::test]
    async fn adds_and_retrieves_spender_leaf() {
        let app = setup_dummy_app().await;
        let channel_context = app
            .config
            .find_chain_token(DUMMY_CAMPAIGN.channel.token)
            .expect("Dummy channel Token should be present in config!")
            .with(DUMMY_CAMPAIGN.channel);

        insert_channel(&app.pool, channel_context.context)
            .await
            .expect("should insert channel");

        let get_accounting_request = |channel_context: &ChainOf<Channel>| {
            Request::builder()
                .extension(channel_context.clone())
                .body(Body::empty())
                .expect("Should build Request")
        };
        let add_spender_request = |channel_context: &ChainOf<Channel>| {
            let param = RouteParams(vec![
                channel_context.context.id().to_string(),
                CREATOR.to_string(),
            ]);
            Request::builder()
                .extension(channel_context.clone())
                .extension(param)
                .body(Body::empty())
                .expect("Should build Request")
        };

        // Calling with non existent accounting
        let res = add_spender_leaf(add_spender_request(&channel_context), &app)
            .await
            .expect("Should add");
        assert_eq!(StatusCode::OK, res.status());

        let res = get_accounting_for_channel(get_accounting_request(&channel_context), &app)
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
        spend_amount(
            app.pool.clone(),
            channel_context.context.id(),
            balances.clone(),
        )
        .await
        .expect("should spend");

        let res = get_accounting_for_channel(get_accounting_request(&channel_context), &app)
            .await
            .expect("should get response");
        assert_eq!(StatusCode::OK, res.status());

        let accounting_response = res_to_accounting_response(res).await;

        assert_eq!(balances, accounting_response.balances);

        let res = add_spender_leaf(add_spender_request(&channel_context), &app)
            .await
            .expect("Should add");
        assert_eq!(StatusCode::OK, res.status());

        let res = get_accounting_for_channel(get_accounting_request(&channel_context), &app)
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
