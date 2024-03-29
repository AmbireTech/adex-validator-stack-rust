//! `/v5/channel` routes
//!

use axum::{extract::Path, Extension, Json};
use futures::future::try_join_all;
use serde::{Deserialize, Serialize};
use slog::{error, Logger};
use std::{any::Any, collections::HashMap, sync::Arc};

use adapter::{
    client::Locked,
    util::{get_balance_leaf, get_signable_state_root},
    Adapter, Dummy,
};
use primitives::{
    balances::{Balances, BalancesState, CheckedState, UncheckedState},
    merkle_tree::MerkleTree,
    sentry::{
        channel_list::{ChannelListQuery, ChannelListResponse},
        message::MessageResponse,
        AccountingResponse, AllSpendersQuery, AllSpendersResponse, ChannelPayRequest,
        GetLeafResponse, LastApproved, LastApprovedQuery, LastApprovedResponse, SpenderResponse,
        SuccessResponse,
    },
    spender::{Spendable, Spender},
    validator::{ApproveState, NewState},
    Address, ChainOf, Channel, ChannelId, Deposit, UnifiedNum,
};

use crate::{
    application::Qs,
    db::{
        accounting::{
            get_accounting, get_all_accountings_for_channel, spend_amount, update_accounting, Side,
        },
        insert_channel, list_channels,
        spendable::{fetch_spendable, get_all_spendables_for_channel, update_spendable},
        validator_message::{latest_approve_state, latest_heartbeats, latest_new_state},
        DbPool,
    },
    response::ResponseError,
    routes::{campaign::fetch_campaign_ids_for_channel, routers::LeafFor},
    Application, Auth,
};

/// Request body for Channel deposit when using the Dummy adapter.
///
/// **NOTE:** available **only** when using the Dummy adapter!
#[derive(Debug, Serialize, Deserialize)]
pub struct ChannelDummyDeposit {
    pub channel: Channel,
    pub deposit: Deposit<UnifiedNum>,
}

/// GET `/v5/channel/list` request
///
/// Request query parameters: [`ChannelListQuery`]
///
/// Response: [`ChannelListResponse`](primitives::sentry::channel_list::ChannelListResponse)
pub async fn channel_list<C: Locked + 'static>(
    Extension(app): Extension<Arc<Application<C>>>,
    Qs(query): Qs<ChannelListQuery>,
) -> Result<Json<ChannelListResponse>, ResponseError> {
    let skip = query
        .page
        .checked_mul(app.config.limits.channels_find.into())
        .ok_or_else(|| ResponseError::BadRequest("Page and/or limit is too large".into()))?;

    let list_response = list_channels(
        &app.pool,
        skip,
        app.config.limits.channels_find,
        query.validator,
        &query.chains,
    )
    .await?;

    Ok(Json(list_response))
}

/// GET `/v5/channel/0xXXX.../last-approved` request
///
/// Full details about the route's API and intend can be found in the [`routes`](crate::routes#get-v5channelidlast-approved) module
///
/// Request query parameters: [`LastApprovedQuery`]
///
/// Response: [`LastApprovedResponse`]
pub async fn last_approved<C: Locked + 'static>(
    Extension(app): Extension<Arc<Application<C>>>,
    Extension(channel_context): Extension<ChainOf<Channel>>,
    Qs(query): Qs<LastApprovedQuery>,
) -> Result<Json<LastApprovedResponse<UncheckedState>>, ResponseError> {
    // get request Channel
    let channel = channel_context.context;

    let default_response = Json(LastApprovedResponse::<UncheckedState> {
        last_approved: None,
        heartbeats: None,
    });

    let (approve_state, new_state) =
        match get_corresponding_states::<UncheckedState>(&app.pool, &app.logger, &channel).await? {
            Some(states) => states,
            None => return Ok(default_response),
        };

    let validators = vec![channel.leader, channel.follower];
    let channel_id = channel.id();

    let heartbeats = if query.with_heartbeat.unwrap_or_default() {
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

    Ok(Json(LastApprovedResponse {
        last_approved: Some(LastApproved {
            new_state: Some(new_state),
            approve_state: Some(approve_state),
        }),
        heartbeats,
    }))
}

/// This will make sure to insert/get the `Channel` from DB before attempting to create the `Spendable`
async fn create_or_update_spendable_document<A: Locked>(
    adapter: &Adapter<A>,
    pool: DbPool,
    channel_context: &ChainOf<Channel>,
    spender: Address,
) -> Result<Spendable, ResponseError> {
    insert_channel(&pool, channel_context).await?;

    let deposit = adapter.get_deposit(channel_context, spender).await?;
    let total = UnifiedNum::from_precision(deposit.total, channel_context.token.precision.get());

    let total = match total {
        Some(total) => total,
        _ => {
            return Err(ResponseError::BadRequest(
                "couldn't get total from precision".to_string(),
            ))
        }
    };

    let spendable = Spendable {
        channel: channel_context.context,
        deposit: Deposit { total },
        spender,
    };

    // Insert latest spendable in DB
    update_spendable(pool, &spendable).await?;

    Ok(spendable)
}

/// GET `/v5/channel/0xXXX.../spender/0xXXX...` request
///
/// Response: [`SpenderResponse`]
pub async fn get_spender_limits<C: Locked + 'static>(
    Path(params): Path<(ChannelId, Address)>,
    Extension(app): Extension<Arc<Application<C>>>,
    Extension(channel_context): Extension<ChainOf<Channel>>,
) -> Result<Json<SpenderResponse>, ResponseError> {
    let channel = &channel_context.context;

    let spender = params.1;

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

    let (_, new_state) =
        match get_corresponding_states::<CheckedState>(&app.pool, &app.logger, channel).await? {
            Some(new_state) => new_state,
            None => {
                return Ok(Json(SpenderResponse {
                    spender: Spender {
                        total_deposited: latest_spendable.deposit.total,
                        total_spent: None,
                    },
                }))
            }
        };

    let total_spent = new_state
        .msg
        .balances
        .spenders
        .get(&spender)
        .map(|spent| spent.to_owned());

    Ok(Json(SpenderResponse {
        spender: Spender {
            total_deposited: latest_spendable.deposit.total,
            total_spent,
        },
    }))
}

/// GET `/v5/channel/0xXXX.../spender/all` request.
///
/// Response: [`AllSpendersResponse`]
pub async fn get_all_spender_limits<C: Locked + 'static>(
    Extension(app): Extension<Arc<Application<C>>>,
    Extension(channel_context): Extension<ChainOf<Channel>>,
    Qs(query): Qs<AllSpendersQuery>,
) -> Result<Json<AllSpendersResponse>, ResponseError> {
    let channel = channel_context.context;

    let limit = app.config.limits.spendable_find;
    let skip = query
        .page
        .checked_mul(limit.into())
        .ok_or_else(|| ResponseError::FailedValidation("Page and/or limit is too large".into()))?;

    let corresponding_states =
        get_corresponding_states::<CheckedState>(&app.pool, &app.logger, &channel).await?;

    let mut all_spender_limits: HashMap<Address, Spender> = HashMap::new();

    let (all_spendables, pagination) =
        get_all_spendables_for_channel(app.pool.clone(), &channel.id(), skip, limit.into()).await?;

    // Using for loop to avoid async closures
    for spendable in all_spendables {
        let spender = spendable.spender;
        let total_spent = corresponding_states.as_ref().and_then(|(_, new_state)| {
            new_state
                .msg
                .balances
                .spenders
                .get(&spender)
                .map(|balance| {
                    spendable
                        .deposit
                        .total
                        .checked_sub(balance)
                        .unwrap_or_default()
                })
        });

        let spender_info = Spender {
            total_deposited: spendable.deposit.total,
            total_spent,
        };

        all_spender_limits.insert(spender, spender_info);
    }

    Ok(Json(AllSpendersResponse {
        spenders: all_spender_limits,
        pagination,
    }))
}

/// POST `/v5/channel/0xXXX.../spender/0xXXX...` request
///
/// Internally to make the validator worker add a spender leaf in `NewState` we'll just update `Accounting`
pub async fn add_spender_leaf<C: Locked + 'static>(
    Extension(app): Extension<Arc<Application<C>>>,
    Extension(channel): Extension<ChainOf<Channel>>,
    Path(params): Path<(ChannelId, Address)>,
) -> Result<Json<SpenderResponse>, ResponseError> {
    let spender = params.1;

    update_accounting(
        app.pool.clone(),
        channel.context.id(),
        spender,
        Side::Spender,
        UnifiedNum::from_u64(0),
    )
    .await?;

    let latest_spendable =
        fetch_spendable(app.pool.clone(), &spender, &channel.context.id()).await?;

    let latest_spendable = match latest_spendable {
        Some(spendable) => spendable,
        None => {
            create_or_update_spendable_document(&app.adapter, app.pool.clone(), &channel, spender)
                .await?
        }
    };

    let (_, new_state) =
        match get_corresponding_states::<CheckedState>(&app.pool, &app.logger, &channel.context)
            .await?
        {
            Some(new_state) => new_state,
            None => {
                return Ok(Json(SpenderResponse {
                    spender: Spender {
                        total_deposited: latest_spendable.deposit.total,
                        total_spent: None,
                    },
                }))
            }
        };

    let total_spent = new_state.msg.balances.spenders.get(&spender).copied();

    Ok(Json(SpenderResponse {
        spender: Spender {
            total_deposited: latest_spendable.deposit.total,
            total_spent,
        },
    }))
}

/// Retrieves the [`Channel`]'s latest [`ApproveState`] and the corresponding
/// approved [`NewState`].
///
/// # Errors
///
/// - Returns [`ResponseError::NotFound`] if there is no [`ApproveState`]
/// generated for the [`Channel`], i.e. the [`Channel`] is new.
///
/// - Returns [`ResponseError::BadRequest`] if the [`ApproveState`]'s
/// corresponding approve [`NewState`] is not found.
///
/// - Returns [`ResponseError::BadRequest`] if the [`NewState`]'s
/// [`Balances`] don't align, i.e. `sum(earner) != sum(spenders)`
///
/// [`ApproveState`]: primitives::validator::ApproveState
async fn get_corresponding_states<S: BalancesState>(
    pool: &DbPool,
    logger: &Logger,
    channel: &Channel,
) -> Result<Option<(MessageResponse<ApproveState>, MessageResponse<NewState<S>>)>, ResponseError> {
    let approve_state = match latest_approve_state(pool, channel).await? {
        Some(approve_state) => approve_state,
        None => return Ok(None),
    };

    let state_root = approve_state.msg.state_root.clone();

    match latest_new_state::<S>(pool, channel, &state_root).await? {
        Some(new_state) => Ok(Some((approve_state, new_state))),
        None => {
            error!(&logger, "{}", "Fatal error! The NewState for the last ApproveState was not found"; "module" => "routes::channel");

            Err(ResponseError::BadRequest(
                "Fatal error! The NewState for the last ApproveState was not found".to_string(),
            ))
        }
    }
}

/// GET `/v5/channel/0xXXX.../accounting` request
///
/// Response: [`AccountingResponse::<CheckedState>`]
pub async fn get_accounting_for_channel<C: Locked + 'static>(
    Extension(app): Extension<Arc<Application<C>>>,
    Extension(channel_context): Extension<ChainOf<Channel>>,
) -> Result<Json<AccountingResponse<CheckedState>>, ResponseError> {
    let channel = channel_context.context;

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

    Ok(Json(AccountingResponse::<CheckedState> { balances }))
}

/// POST `/v5/channel/0xXXX.../pay` request
///
/// Body: [`ChannelPayRequest`]
///
/// Response: [`SuccessResponse`]
pub async fn channel_payout<C: Locked + 'static>(
    Extension(app): Extension<Arc<Application<C>>>,
    Extension(channel_context): Extension<ChainOf<Channel>>,
    Extension(auth): Extension<Auth>,
    Json(to_pay): Json<ChannelPayRequest>,
) -> Result<Json<SuccessResponse>, ResponseError> {
    let spender = auth.uid.to_address();

    // Handling the case where a request with an empty body comes through
    if to_pay.payouts.is_empty() {
        return Err(ResponseError::FailedValidation(
            "Request has empty payouts".to_string(),
        ));
    }

    let channel_campaigns = fetch_campaign_ids_for_channel(
        &app.pool,
        channel_context.context.id(),
        app.config.limits.campaigns_find,
    )
    .await?;

    let campaigns_remaining_sum = app
        .campaign_remaining
        .get_multiple(&channel_campaigns)
        .await?
        .iter()
        .sum::<Option<UnifiedNum>>()
        .ok_or_else(|| {
            ResponseError::BadRequest("Couldn't sum remaining amount for all campaigns".to_string())
        })?;

    // A campaign is closed when its remaining == 0
    // therefore for all campaigns for a channel to be closed their total remaining sum should be 0
    if campaigns_remaining_sum > UnifiedNum::from_u64(0) {
        return Err(ResponseError::FailedValidation(
            "All campaigns should be closed or have no budget left".to_string(),
        ));
    }

    let accounting_spent = get_accounting(
        app.pool.clone(),
        channel_context.context.id(),
        spender,
        Side::Spender,
    )
    .await?
    .map(|accounting_spent| accounting_spent.amount)
    .unwrap_or_default();
    let accounting_earned = get_accounting(
        app.pool.clone(),
        channel_context.context.id(),
        spender,
        Side::Earner,
    )
    .await?
    .map(|accounting_spent| accounting_spent.amount)
    .unwrap_or_default();
    let latest_spendable =
        fetch_spendable(app.pool.clone(), &spender, &channel_context.context.id())
            .await
            .map_err(|err| ResponseError::BadRequest(err.to_string()))?
            .ok_or_else(|| {
                ResponseError::BadRequest(
                    "There is no spendable amount for the spender in this Channel".to_string(),
                )
            })?;
    let total_deposited = latest_spendable.deposit.total;

    let available_for_payout = total_deposited
        .checked_add(&accounting_earned)
        .ok_or_else(|| {
            ResponseError::FailedValidation(
                "Overflow while calculating available for payout".to_string(),
            )
        })?
        .checked_sub(&accounting_spent)
        .ok_or_else(|| {
            ResponseError::FailedValidation(
                "Underflow while calculating available for payout".to_string(),
            )
        })?;

    let total_to_pay = to_pay
        .payouts
        .values()
        .sum::<Option<UnifiedNum>>()
        .ok_or_else(|| ResponseError::FailedValidation("Payouts amount overflow".to_string()))?;

    if total_to_pay > available_for_payout {
        return Err(ResponseError::FailedValidation(
            "The total requested payout amount exceeds the available payout".to_string(),
        ));
    }

    let mut balances: Balances<CheckedState> = Balances::new();
    for (earner, amount) in to_pay.payouts.into_iter() {
        balances.spend(spender, earner, amount)?;
    }

    // will return an error if one of the updates fails
    spend_amount(app.pool.clone(), channel_context.context.id(), balances).await?;

    Ok(Json(SuccessResponse { success: true }))
}

/// GET `/v5/channel/0xXXX.../get-leaf` requests
///
/// # Routes:
///
/// - GET `/v5/channel/:id/get-leaf/spender/:addr`
/// - GET `/v5/channel/:id/get-leaf/earner/:addr`
///
/// Response: [`GetLeafResponse`]
pub async fn get_leaf<C: Locked + 'static>(
    Extension(app): Extension<Arc<Application<C>>>,
    Extension(channel_context): Extension<ChainOf<Channel>>,
    Extension(leaf_for): Extension<LeafFor>,
    Path(params): Path<(ChannelId, Address)>,
) -> Result<Json<GetLeafResponse>, ResponseError> {
    let channel = channel_context.context;

    let (_, new_state) = get_corresponding_states::<CheckedState>(&app.pool, &app.logger, &channel)
        .await?
        .ok_or(ResponseError::NotFound)?;

    let addr = params.1;

    let element = match leaf_for {
        LeafFor::Spender => {
            let amount = new_state
                .msg
                .balances
                .spenders
                .get(&addr)
                .ok_or(ResponseError::NotFound)?;

            get_balance_leaf(
                true,
                &addr,
                &amount.to_precision(channel_context.token.precision.get()),
            )?
        }
        LeafFor::Earner => {
            let amount = new_state
                .msg
                .balances
                .earners
                .get(&addr)
                .ok_or(ResponseError::NotFound)?;

            get_balance_leaf(
                false,
                &addr,
                &amount.to_precision(channel_context.token.precision.get()),
            )?
        }
    };
    let merkle_tree = MerkleTree::new(&[element])?;

    let signable_state_root = get_signable_state_root(channel.id().as_bytes(), &merkle_tree.root());

    let merkle_proof = hex::encode(signable_state_root);

    Ok(Json(GetLeafResponse { merkle_proof }))
}

/// POST `/v5/channel/dummy-deposit` request
///
/// Full details about the route's API and intend can be found in the [`routes`](crate::routes#post-v5channeldummy-deposit-auth-required) module
///
/// Request body (json): [`ChannelDummyDeposit`]
///
/// Response: [`SuccessResponse`]
pub async fn channel_dummy_deposit<C: Locked + 'static>(
    Extension(app): Extension<Arc<Application<C>>>,
    Extension(auth): Extension<Auth>,
    Json(request): Json<ChannelDummyDeposit>,
) -> Result<Json<SuccessResponse>, ResponseError> {
    let depositor = auth.uid.to_address();

    // Insert the channel if it does not exist yet
    let channel_chain = app
        .config
        .find_chain_of(request.channel.token)
        .expect("The Chain of Channel's token was not found in configuration!")
        .with_channel(request.channel);

    // if this fails, it will cause Bad Request
    insert_channel(&app.pool, &channel_chain).await?;

    // Convert the UnifiedNum to BigNum with the precision of the token
    let deposit = request
        .deposit
        .to_precision(channel_chain.token.precision.into());

    let dummy_adapter = <dyn Any + Send + Sync>::downcast_ref::<Adapter<Dummy>>(&app.adapter)
        .expect("Only Dummy adapter is allowed here!");

    // set the deposit in the Dummy adapter's client
    dummy_adapter
        .client
        .set_deposit(&channel_chain, depositor, deposit);

    Ok(Json(SuccessResponse { success: true }))
}

/// [`Channel`] [validator messages](primitives::validator::MessageTypes) routes
/// starting with `/v5/channel/0xXXX.../validator-messages`
///
pub mod validator_message {
    use std::sync::Arc;

    use axum::{extract::Path, Extension, Json};
    use futures::future::try_join_all;
    use serde::Deserialize;

    use adapter::client::Locked;
    use primitives::{
        sentry::{
            validator_messages::{
                MessageTypesFilter, ValidatorMessagesCreateRequest, ValidatorMessagesListQuery,
                ValidatorMessagesListResponse,
            },
            SuccessResponse,
        },
        ChainOf, Channel, ChannelId, ValidatorId,
    };

    use crate::{
        application::Qs,
        db::validator_message::{get_validator_messages, insert_validator_message},
        response::ResponseError,
        Application, Auth,
    };

    #[derive(Debug, Deserialize)]
    pub struct MessagesListParams {
        pub id: ChannelId,
        // Optional filtering by [`ValidatorId`].
        #[serde(default)]
        pub address: Option<ValidatorId>,
        /// Optional filtering by a list of [`MessageType`](primitives::validator::messages::MessageType)s
        /// separated by `+` (url encoded).
        ///
        /// See [`MessageTypesFilter`] for more details.
        #[serde(default)]
        pub message_types: MessageTypesFilter,
    }

    /// GET `/v5/channel/0xXXX.../validator-messages`
    ///
    /// Full details about the route's API and intend can be found in the [`routes`](crate::routes#get-v5channelidvalidator-messages) module
    ///
    /// Request query parameters: [`ValidatorMessagesListQuery`]
    ///
    /// Response: [`ValidatorMessagesListResponse`]
    ///
    pub async fn list_validator_messages<C: Locked + 'static>(
        Extension(app): Extension<Arc<Application<C>>>,
        Extension(channel_context): Extension<ChainOf<Channel>>,
        Path(params): Path<MessagesListParams>,
        Qs(query): Qs<ValidatorMessagesListQuery>,
    ) -> Result<Json<ValidatorMessagesListResponse>, ResponseError> {
        let channel = channel_context.context;

        let config_limit = app.config.limits.msgs_find as u64;
        let limit = query
            .limit
            .filter(|n| *n >= 1)
            .unwrap_or(config_limit)
            .min(config_limit);

        let validator_messages = get_validator_messages(
            &app.pool,
            &channel.id(),
            &params.address,
            params.message_types.as_ref(),
            limit,
        )
        .await?;

        Ok(Json(ValidatorMessagesListResponse {
            messages: validator_messages,
        }))
    }

    /// POST `/v5/channel/0xXXX.../validator-messages`
    ///
    /// Full details about the route's API and intend can be found in the [`routes`](crate::routes#post-v5channelidvalidator-messages-auth-required) module
    ///
    /// Request body (json): [`ValidatorMessagesCreateRequest`]
    ///
    /// Response: [`SuccessResponse`]
    ///
    /// # Examples
    ///
    /// Request:
    ///
    /// ```
    #[doc = include_str!("../../../primitives/examples/validator_messages_create_request.rs")]
    /// ```
    pub async fn create_validator_messages<C: Locked + 'static>(
        Extension(app): Extension<Arc<Application<C>>>,
        Extension(auth): Extension<Auth>,
        Extension(channel_context): Extension<ChainOf<Channel>>,
        Json(create_request): Json<ValidatorMessagesCreateRequest>,
    ) -> Result<Json<SuccessResponse>, ResponseError> {
        let channel = channel_context.context;

        match channel.find_validator(auth.uid) {
            None => Err(ResponseError::Unauthorized),
            _ => {
                try_join_all(create_request.messages.iter().map(|message| {
                    insert_validator_message(&app.pool, &channel, &auth.uid, message)
                }))
                .await?;

                Ok(Json(SuccessResponse { success: true }))
            }
        }
    }

    #[cfg(test)]
    mod test {
        use super::*;
        use crate::{
            db::{insert_channel, validator_message::get_validator_messages},
            test_util::setup_dummy_app,
        };

        use adapter::prelude::Unlocked;
        use chrono::Utc;

        use primitives::{
            balances::UncheckedState,
            sentry::validator_messages::ValidatorMessagesCreateRequest,
            test_util::{ADVERTISER, CAMPAIGNS, IDS, LEADER},
            validator::{
                ApproveState, Heartbeat, MessageType, MessageTypes, NewState, RejectState,
            },
            Balances,
        };

        #[tokio::test]
        async fn post_validator_messages() {
            let app_guard = setup_dummy_app().await;
            let app = Extension(Arc::new(app_guard.app.clone()));
            let all_message_types: [MessageType; 4] = [
                MessageType::NewState,
                MessageType::ApproveState,
                MessageType::RejectState,
                MessageType::Heartbeat,
            ];

            let channel_context = Extension(CAMPAIGNS[0].clone().of_channel());

            insert_channel(&app.pool, &channel_context)
                .await
                .expect("should insert channel");

            // Case when the request is not sent by a channel validator
            {
                let auth = Auth {
                    era: 0,
                    uid: IDS[&ADVERTISER],
                    chain: channel_context.chain.clone(),
                };

                let req = ValidatorMessagesCreateRequest { messages: vec![] };

                let res = create_validator_messages(
                    app.clone(),
                    Extension(auth),
                    channel_context.clone(),
                    Json(req),
                )
                .await
                .expect_err("We expect an error to be returned");

                assert_eq!(res, ResponseError::Unauthorized);
            }

            // Cases where we insert validator messages
            // Case with 0 messages
            {
                let auth = Auth {
                    era: 0,
                    uid: IDS[&LEADER],
                    chain: channel_context.chain.clone(),
                };

                let req = ValidatorMessagesCreateRequest { messages: vec![] };

                create_validator_messages(
                    app.clone(),
                    Extension(auth),
                    channel_context.clone(),
                    Json(req),
                )
                .await
                .expect("should work");

                let messages = get_validator_messages(
                    &app.pool,
                    &channel_context.context.id(),
                    &Some(IDS[&LEADER]),
                    &all_message_types,
                    100,
                )
                .await
                .expect("should get messages");

                assert_eq!(messages.len(), 0);
            }
            // Case with 1 message
            {
                let auth = Auth {
                    era: 0,
                    uid: IDS[&LEADER],
                    chain: channel_context.chain.clone(),
                };

                let message = MessageTypes::Heartbeat(Heartbeat {
                    signature: String::new(),
                    state_root: String::new(),
                    timestamp: Utc::now(),
                });

                let req = ValidatorMessagesCreateRequest {
                    messages: vec![message.clone()],
                };

                create_validator_messages(
                    app.clone(),
                    Extension(auth),
                    channel_context.clone(),
                    Json(req),
                )
                .await
                .expect("Should create messages");

                let messages = get_validator_messages(
                    &app.pool,
                    &channel_context.context.id(),
                    &Some(IDS[&LEADER]),
                    &all_message_types,
                    100,
                )
                .await
                .expect("should get messages");
                let inner_messages: Vec<MessageTypes> =
                    messages.into_iter().map(|m| m.msg.clone()).collect();
                assert_eq!(inner_messages, vec![message]);
            }
            // Case with 4 messages (one of each type, heartbeat is already inserted)
            {
                let auth = Auth {
                    era: 0,
                    uid: IDS[&LEADER],
                    chain: channel_context.chain.clone(),
                };

                let state_root =
                    "b1a4fc6c1a1e1ab908a487e504006edcebea297f61b4b8ce6cad3b29e29454cc".to_string();
                let signature = app
                    .adapter
                    .clone()
                    .unlock()
                    .expect("should unlock")
                    .sign(&state_root.clone())
                    .expect("should sign");
                let new_state = MessageTypes::NewState(NewState {
                    state_root: state_root.clone(),
                    signature: signature.clone(),
                    balances: Balances::<UncheckedState>::new(),
                });

                let approve_state = MessageTypes::ApproveState(ApproveState {
                    state_root: state_root.clone(),
                    signature: signature.clone(),
                    is_healthy: true,
                });

                let reject_state = MessageTypes::RejectState(RejectState {
                    state_root,
                    signature: signature.clone(),
                    timestamp: Utc::now(),
                    reason: "rejected".to_string(),
                    balances: None,
                });

                let req = ValidatorMessagesCreateRequest {
                    messages: vec![
                        new_state.clone(),
                        approve_state.clone(),
                        reject_state.clone(),
                    ],
                };

                create_validator_messages(
                    app.clone(),
                    Extension(auth),
                    channel_context.clone(),
                    Json(req),
                )
                .await
                .expect("Should create messages");

                let messages = get_validator_messages(
                    &app.pool,
                    &channel_context.context.id(),
                    &Some(IDS[&LEADER]),
                    &all_message_types,
                    100,
                )
                .await
                .expect("should get messages");
                assert_eq!(messages.len(), 4);
            }
        }
    }
}

#[cfg(test)]
mod test {
    use std::str::FromStr;

    use super::*;
    use crate::{
        db::{
            insert_campaign, insert_channel, validator_message::insert_validator_message,
            CampaignRemaining,
        },
        test_util::setup_dummy_app,
    };

    use adapter::{
        ethereum::test_util::{GANACHE_INFO_1, GANACHE_INFO_1337},
        prelude::Unlocked,
        primitives::Deposit as AdapterDeposit,
    };
    use primitives::{
        balances::UncheckedState,
        channel::Nonce,
        test_util::{
            ADVERTISER, CREATOR, DUMMY_CAMPAIGN, FOLLOWER, GUARDIAN, IDS, LEADER, LEADER_2,
            PUBLISHER, PUBLISHER_2,
        },
        validator::{ApproveState, MessageTypes, NewState},
        BigNum, ChainId, Deposit, UnifiedMap, ValidatorId,
    };

    #[tokio::test]
    async fn create_and_fetch_spendable() {
        let app = setup_dummy_app().await;

        let (channel_context, channel) = {
            let channel = DUMMY_CAMPAIGN.channel;
            let channel_context = app
                .config
                .find_chain_of(DUMMY_CAMPAIGN.channel.token)
                .expect("should retrieve Chain & token");

            (channel_context.with_channel(channel), channel)
        };

        let precision: u8 = channel_context.token.precision.into();
        let deposit = AdapterDeposit {
            total: BigNum::from_str("100000000000000000000").expect("should convert"), // 100 DAI
        };
        app.adapter
            .client
            .set_deposit(&channel_context, *CREATOR, deposit.clone());
        // Making sure spendable does not yet exist
        let spendable = fetch_spendable(app.pool.clone(), &CREATOR, &channel.id())
            .await
            .expect("should return None");
        assert!(spendable.is_none());
        // Call create_or_update_spendable
        let new_spendable = create_or_update_spendable_document(
            &app.adapter,
            app.pool.clone(),
            &channel_context,
            *CREATOR,
        )
        .await
        .expect("should create a new spendable");
        assert_eq!(new_spendable.channel.id(), channel.id());

        let total_as_unified_num =
            UnifiedNum::from_precision(deposit.total, precision).expect("should convert");

        assert_eq!(new_spendable.deposit.total, total_as_unified_num);

        assert_eq!(new_spendable.spender, *CREATOR);

        // Make sure spendable NOW exists
        let spendable = fetch_spendable(app.pool.clone(), &CREATOR, &channel.id())
            .await
            .expect("should return a spendable");
        assert!(spendable.is_some());

        let updated_deposit = AdapterDeposit {
            total: BigNum::from_str("110000000000000000000").expect("should convert"), // 110 DAI
        };

        app.adapter
            .client
            .set_deposit(&channel_context, *CREATOR, updated_deposit.clone());

        let updated_spendable = create_or_update_spendable_document(
            &app.adapter,
            app.pool.clone(),
            &channel_context,
            *CREATOR,
        )
        .await
        .expect("should update spendable");
        let total_as_unified_num =
            UnifiedNum::from_precision(updated_deposit.total, precision).expect("should convert");

        assert_eq!(updated_spendable.deposit.total, total_as_unified_num);
        assert_eq!(updated_spendable.spender, *CREATOR);
    }

    #[tokio::test]
    async fn get_accountings_for_channel() {
        let app_guard = setup_dummy_app().await;

        let app = Extension(Arc::new(app_guard.app.clone()));
        let channel_context = app
            .config
            .find_chain_of(DUMMY_CAMPAIGN.channel.token)
            .expect("Dummy channel Token should be present in config!")
            .with(DUMMY_CAMPAIGN.channel);

        insert_channel(&app.pool, &channel_context)
            .await
            .expect("should insert channel");

        // Testing for no accounting yet
        {
            let accounting_response =
                get_accounting_for_channel(app.clone(), Extension(channel_context.clone()))
                    .await
                    .expect("shoul get accounting");

            assert_eq!(accounting_response.balances.earners.len(), 0);
            assert_eq!(accounting_response.balances.spenders.len(), 0);
        }

        // Testing for 2 accountings - first channel
        {
            let mut balances = Balances::<CheckedState>::new();
            balances
                .spend(*CREATOR, *PUBLISHER, UnifiedNum::from_u64(200))
                .expect("should not overflow");
            balances
                .spend(*CREATOR, *PUBLISHER_2, UnifiedNum::from_u64(100))
                .expect("Should not overflow");
            spend_amount(
                app.pool.clone(),
                channel_context.context.id(),
                balances.clone(),
            )
            .await
            .expect("should spend");

            let accounting_response =
                get_accounting_for_channel(app.clone(), Extension(channel_context.clone()))
                    .await
                    .expect("should get accounting");

            assert_eq!(balances, accounting_response.balances);
        }

        // Testing for 2 accountings - second channel (same address is both an earner and a spender)
        {
            let mut second_channel = DUMMY_CAMPAIGN.channel;
            second_channel.leader = IDS[&ADVERTISER]; // channel.id() will be different now
            let channel_context = app
                .config
                .find_chain_of(second_channel.token)
                .expect("Dummy channel Token should be present in config!")
                .with(second_channel);
            insert_channel(&app.pool, &channel_context)
                .await
                .expect("should insert channel");

            let mut balances = Balances::<CheckedState>::new();
            balances
                .spend(*CREATOR, *PUBLISHER, 300.into())
                .expect("Should not overflow");

            balances
                .spend(*PUBLISHER, *ADVERTISER, 300.into())
                .expect("Should not overflow");

            spend_amount(app.pool.clone(), second_channel.id(), balances.clone())
                .await
                .expect("should spend");

            let accounting_response =
                get_accounting_for_channel(app.clone(), Extension(channel_context.clone()))
                    .await
                    .expect("shoul get accounting");

            assert_eq!(balances, accounting_response.balances)
        }

        // Testing for when sums don't match on first channel - Error case
        {
            let mut balances = Balances::<CheckedState>::new();
            balances
                .earners
                .insert(*PUBLISHER, UnifiedNum::from_u64(100));
            balances
                .spenders
                .insert(*CREATOR, UnifiedNum::from_u64(200));
            spend_amount(app.pool.clone(), channel_context.context.id(), balances)
                .await
                .expect("should spend");

            let res =
                get_accounting_for_channel(app.clone(), Extension(channel_context.clone())).await;
            let expected = ResponseError::FailedValidation(
                "Earners sum is not equal to spenders sum for channel".to_string(),
            );
            assert_eq!(expected, res.expect_err("Should return an error"));
        }
    }

    #[tokio::test]
    async fn adds_and_retrieves_spender_leaf() {
        let app_guard = setup_dummy_app().await;

        let app = Extension(Arc::new(app_guard.app.clone()));

        let channel_context = app
            .config
            .find_chain_of(DUMMY_CAMPAIGN.channel.token)
            .expect("Dummy channel Token should be present in config!")
            .with(DUMMY_CAMPAIGN.channel);

        let deposit = AdapterDeposit {
            total: BigNum::from_str("100000000000000000000").expect("should convert"), // 100 DAI
        };
        app.adapter
            .client
            .set_deposit(&channel_context, *CREATOR, deposit.clone());

        insert_channel(&app.pool, &channel_context)
            .await
            .expect("should insert channel");

        // Calling with non existent accounting
        let _res = add_spender_leaf(
            app.clone(),
            Extension(channel_context.clone()),
            Path((channel_context.context.id(), *CREATOR)),
        )
        .await
        .expect("should get spender leaf");

        let accounting_response =
            get_accounting_for_channel(app.clone(), Extension(channel_context.clone()))
                .await
                .expect("should get accounting");

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

        let accounting_response =
            get_accounting_for_channel(app.clone(), Extension(channel_context.clone()))
                .await
                .expect("should get accounting");

        assert_eq!(balances, accounting_response.balances);

        let res = add_spender_leaf(
            app.clone(),
            Extension(channel_context.clone()),
            Path((channel_context.context.id(), *CREATOR)),
        )
        .await;
        assert!(res.is_ok());

        let accounting_response =
            get_accounting_for_channel(app.clone(), Extension(channel_context.clone()))
                .await
                .expect("should get accounting");

        // Balances shouldn't change
        assert_eq!(
            accounting_response.balances.spenders.get(&CREATOR),
            balances.spenders.get(&CREATOR),
        );
    }

    #[tokio::test]
    async fn get_channels_list() {
        let mut app_guard = setup_dummy_app().await;
        app_guard.config.limits.channels_find = 2;

        let app = Extension(Arc::new(app_guard.app.clone()));

        let channel = Channel {
            leader: IDS[&LEADER],
            follower: IDS[&FOLLOWER],
            guardian: *GUARDIAN,
            token: GANACHE_INFO_1337.tokens["Mocked TOKEN 1337"].address,
            nonce: Nonce::from(987_654_321_u32),
        };
        let channel_context = app
            .config
            .find_chain_of(channel.token)
            .expect("Dummy channel Token should be present in config!")
            .with(channel);
        insert_channel(&app.pool, &channel_context)
            .await
            .expect("should insert");

        let channel_other_token = Channel {
            leader: IDS[&LEADER],
            follower: IDS[&FOLLOWER],
            guardian: *GUARDIAN,
            token: GANACHE_INFO_1.tokens["Mocked TOKEN 1"].address,
            nonce: Nonce::from(987_654_322_u32),
        };
        let channel_context = app
            .config
            .find_chain_of(channel_other_token.token)
            .expect("Dummy channel Token should be present in config!")
            .with(channel_other_token);
        insert_channel(&app.pool, &channel_context)
            .await
            .expect("should insert");

        let channel_other_leader = Channel {
            leader: IDS[&LEADER_2],
            follower: IDS[&FOLLOWER],
            guardian: *GUARDIAN,
            token: GANACHE_INFO_1337.tokens["Mocked TOKEN 1337"].address,
            nonce: Nonce::from(987_654_323_u32),
        };
        let channel_context = app
            .config
            .find_chain_of(channel_other_leader.token)
            .expect("Dummy channel Token should be present in config!")
            .with(channel_other_leader);
        insert_channel(&app.pool, &channel_context)
            .await
            .expect("should insert");

        // Test query page - page 0, page 1
        {
            let query = ChannelListQuery {
                page: 0,
                validator: None,
                chains: vec![],
            };

            let channels_list = channel_list(app.clone(), Qs(query))
                .await
                .expect("should get channels")
                .0;

            assert_eq!(
                channels_list.channels,
                vec![channel, channel_other_token],
                "The channels should be listed by ascending order of their creation"
            );
            assert_eq!(
                channels_list.pagination.total_pages, 2,
                "There should be 2 pages in total"
            );

            let query = ChannelListQuery {
                page: 1,
                validator: None,
                chains: vec![],
            };
            let channels_list = channel_list(app.clone(), Qs(query))
                .await
                .expect("should get channels")
                .0;

            assert_eq!(
                channels_list.channels,
                vec![channel_other_leader],
                "The channels should be listed by ascending order of their creation"
            );
        }

        // Test query validator - query with validator ID
        {
            let query = ChannelListQuery {
                page: 0,
                validator: Some(IDS[&LEADER_2]),
                chains: vec![],
            };
            let channels_list = channel_list(app.clone(), Qs(query))
                .await
                .expect("should get channels")
                .0;

            assert_eq!(
                channels_list.channels,
                vec![channel_other_leader],
                "Response returns the correct channel"
            );
            assert_eq!(
                channels_list.pagination.total_pages, 1,
                "There should be 1 page in total"
            );
        }
        // Test query with both pagination and validator by querying for the follower validator
        {
            let query = ChannelListQuery {
                page: 0,
                validator: Some(IDS[&FOLLOWER]),
                chains: vec![],
            };
            let channels_list = channel_list(app.clone(), Qs(query))
                .await
                .expect("should get channels");

            assert_eq!(
                channels_list.pagination.total_pages, 2,
                "There should be 2 pages in total"
            );
            assert_eq!(
                channels_list.channels,
                vec![channel, channel_other_token],
                "The channels should be listed by ascending order of their creation"
            );

            let query = ChannelListQuery {
                page: 1,
                validator: Some(IDS[&FOLLOWER]),
                chains: vec![],
            };
            let channels_list = channel_list(app.clone(), Qs(query))
                .await
                .expect("should get channels");

            assert_eq!(
                channels_list.channels,
                vec![channel_other_leader],
                "The channels should be listed by ascending order of their creation"
            );
            assert_eq!(
                channels_list.pagination.total_pages, 2,
                "There should be 2 pages in total"
            );
        }

        // Test query with different chains
        {
            let limit_app = {
                let mut limit_app = app_guard.app;
                limit_app.config.limits.channels_find = 10; // no need to test pagination, will ease checking results for this cause

                Extension(Arc::new(limit_app))
            };

            let query_1 = ChannelListQuery {
                page: 0,
                validator: Some(IDS[&FOLLOWER]),
                chains: vec![ChainId::new(1)],
            };

            let channels_list = channel_list(limit_app.clone(), Qs(query_1))
                .await
                .expect("should get channels");

            assert_eq!(
                channels_list.channels,
                vec![channel_other_token],
                "Response returns the correct channel"
            );

            let query_1337 = ChannelListQuery {
                page: 0,
                validator: Some(IDS[&FOLLOWER]),
                chains: vec![ChainId::new(1337)],
            };

            let channels_list = channel_list(limit_app.clone(), Qs(query_1337))
                .await
                .expect("should get channels");

            assert_eq!(
                channels_list.channels,
                vec![channel, channel_other_leader],
                "Response returns the correct channel"
            );

            let query_both_chains = ChannelListQuery {
                page: 0,
                validator: Some(IDS[&FOLLOWER]),
                chains: vec![ChainId::new(1), ChainId::new(1337)],
            };

            let channels_list = channel_list(limit_app, Qs(query_both_chains))
                .await
                .expect("should get channels");

            assert_eq!(
                channels_list.channels,
                vec![channel, channel_other_token, channel_other_leader],
                "Response returns the correct channel"
            );
        }
    }

    #[tokio::test]
    async fn payouts_for_earners_test() {
        let app_guard = setup_dummy_app().await;
        let app = Extension(Arc::new(app_guard.app.clone()));

        let channel_context = Extension(
            app.config
                .find_chain_of(DUMMY_CAMPAIGN.channel.token)
                .expect("Dummy channel Token should be present in config!")
                .with(DUMMY_CAMPAIGN.channel),
        );

        insert_channel(&app.pool, &channel_context)
            .await
            .expect("should insert channel");
        insert_campaign(&app.pool, &DUMMY_CAMPAIGN)
            .await
            .expect("should insert the campaign");

        // Setting the initial remaining to 0
        let campaign_remaining = CampaignRemaining::new(app.redis.clone());
        campaign_remaining
            .set_initial(DUMMY_CAMPAIGN.id, UnifiedNum::from_u64(0))
            .await
            .expect("Should set value in redis");

        let auth = Extension(Auth {
            era: 0,
            uid: ValidatorId::from(DUMMY_CAMPAIGN.creator),
            chain: channel_context.chain.clone(),
        });

        let mut payouts = UnifiedMap::default();
        payouts.insert(*PUBLISHER, UnifiedNum::from_u64(500));
        let to_pay = Json(ChannelPayRequest { payouts });

        // Testing before Accounting/Spendable are inserted
        {
            let err_response = channel_payout(
                app.clone(),
                channel_context.clone(),
                auth.clone(),
                to_pay.clone(),
            )
            .await
            .expect_err("Should return an error when there is no Accounting/Spendable");
            assert_eq!(
                err_response,
                ResponseError::BadRequest(
                    "There is no spendable amount for the spender in this Channel".to_string()
                ),
                "Failed validation because payouts are empty"
            );
        }

        // Add accounting for spender = 500
        update_accounting(
            app_guard.pool.clone(),
            channel_context.context.id(),
            auth.uid.to_address(),
            Side::Spender,
            UnifiedNum::from_u64(500),
        )
        .await
        .expect("should update");

        // Add spendable for the channel_context where total deposit = 1000
        let spendable = Spendable {
            spender: auth.uid.to_address(),
            channel: channel_context.context,
            deposit: Deposit {
                total: UnifiedNum::from_u64(1000),
            },
        };

        // Add accounting for earner = 100
        // available for return will be = 600
        update_accounting(
            app_guard.pool.clone(),
            channel_context.context.id(),
            auth.uid.to_address(),
            Side::Earner,
            UnifiedNum::from_u64(100),
        )
        .await
        .expect("should update");

        // Updating spendable so that we have a value for total_deposited
        update_spendable(app_guard.pool.clone(), &spendable)
            .await
            .expect("Should update spendable");

        // Test with empty payouts
        {
            let to_pay = Json(ChannelPayRequest {
                payouts: UnifiedMap::default(),
            });
            let err_response =
                channel_payout(app.clone(), channel_context.clone(), auth.clone(), to_pay)
                    .await
                    .expect_err("Should return an error when payouts are empty");

            assert_eq!(
                err_response,
                ResponseError::FailedValidation("Request has empty payouts".to_string()),
                "Failed validation because payouts are empty"
            );
        }

        // make a normal request and get accounting to see if its as expected
        {
            let success_response = channel_payout(
                app.clone(),
                channel_context.clone(),
                auth.clone(),
                to_pay.clone(),
            )
            .await
            .expect("This request shouldn't result in an error");

            assert_eq!(
                SuccessResponse { success: true },
                success_response.0,
                "Request with JSON body with one address and no errors triggered on purpose"
            );
        }

        // Checking if Earner/Spender balances have been updated by looking up the Accounting in the database
        {
            let earner_accounting = get_accounting(
                app_guard.pool.clone(),
                channel_context.context.id(),
                *PUBLISHER,
                Side::Earner,
            )
            .await
            .expect("should get accounting")
            .expect("Should have value, i.e. Some");
            assert_eq!(
                earner_accounting.amount,
                UnifiedNum::from_u64(500),
                "Accounting is updated to reflect the publisher's earnings"
            );

            let spender_accounting = get_accounting(
                app_guard.pool.clone(),
                channel_context.context.id(),
                auth.uid.to_address(),
                Side::Spender,
            )
            .await
            .expect("should get accounting")
            .expect("Should have value, i.e. Some");

            assert_eq!(
                spender_accounting.amount,
                UnifiedNum::from_u64(1000), // 500 initial + 500 for earners
                "Accounting is updated to reflect the amount deducted from the spender"
            );
        }

        // make a request where "total_to_pay" will exceed available
        {
            let mut payouts = to_pay.payouts.clone();
            payouts.insert(*CREATOR, UnifiedNum::from_u64(1000));
            let to_pay_exceed = Json(ChannelPayRequest { payouts });

            let response_error = channel_payout(
                app.clone(),
                channel_context.clone(),
                auth.clone(),
                to_pay_exceed,
            )
            .await
            .expect_err("Should return an error when total_to_pay > available_for_payout");

            assert_eq!(
                ResponseError::FailedValidation(
                    "The total requested payout amount exceeds the available payout".to_string()
                ),
                response_error,
                "Failed validation because total_to_pay > available_for_payout"
            );
        }

        // make a request where total - spent + earned will be a negative balance resulting in an error
        {
            update_accounting(
                app_guard.pool.clone(),
                channel_context.context.id(),
                auth.uid.to_address(),
                Side::Spender,
                UnifiedNum::from_u64(1000),
            )
            .await
            .expect("should update"); // total spent: 500 + 1000

            let response_error = channel_payout(
                app.clone(),
                channel_context.clone(),
                auth.clone(),
                to_pay.clone(),
            )
            .await
            .expect_err("Should return err when available_for_payout is negative");

            assert_eq!(
                ResponseError::FailedValidation(
                    "Underflow while calculating available for payout".to_string()
                ),
                response_error,
                "Failed validation because available_for_payout is negative"
            );
        }

        // make a request where campaigns will have available remaining
        {
            campaign_remaining
                .increase_by(DUMMY_CAMPAIGN.id, UnifiedNum::from_u64(1000))
                .await
                .expect("Should set value in redis");

            let response_error = channel_payout(app, channel_context, auth, to_pay)
                .await
                .expect_err("Should return an error when a campaign has remaining funds");

            assert_eq!(
                ResponseError::FailedValidation(
                    "All campaigns should be closed or have no budget left".to_string()
                ),
                response_error,
                "Failed validation because the campaign has remaining funds"
            );
        }
    }

    #[tokio::test]
    async fn get_spender_and_earner_leafs() {
        let mut balances: Balances<CheckedState> = Balances::new();
        balances
            .spend(*ADVERTISER, *PUBLISHER, UnifiedNum::from_u64(1000))
            .expect("should spend");
        balances
            .spend(*ADVERTISER, *PUBLISHER_2, UnifiedNum::from_u64(1000))
            .expect("should spend");
        balances
            .spend(*CREATOR, *PUBLISHER, UnifiedNum::from_u64(1000))
            .expect("should spend");
        balances
            .spend(*CREATOR, *PUBLISHER_2, UnifiedNum::from_u64(1000))
            .expect("should spend");

        let app_guard = setup_dummy_app().await;
        let app = Extension(Arc::new(app_guard.app.clone()));

        let channel_context = Extension(
            app.config
                .find_chain_of(DUMMY_CAMPAIGN.channel.token)
                .expect("Dummy channel Token should be present in config!")
                .with(DUMMY_CAMPAIGN.channel),
        );
        let channel = channel_context.context;

        insert_channel(&app.pool, &channel_context)
            .await
            .expect("should insert channel");

        // Setting up the validator messages
        let state_root =
            "b1a4fc6c1a1e1ab908a487e504006edcebea297f61b4b8ce6cad3b29e29454cc".to_string();
        let signature = app
            .adapter
            .clone()
            .unlock()
            .expect("should unlock")
            .sign(&state_root.clone())
            .expect("should sign");
        let new_state: NewState<UncheckedState> = NewState {
            state_root: state_root.clone(),
            signature: signature.clone(),
            balances: balances.into_unchecked(),
        };
        let approve_state = ApproveState {
            state_root,
            signature,
            is_healthy: true,
        };

        insert_validator_message(
            &app.pool,
            &channel,
            &channel.leader,
            &MessageTypes::NewState(new_state),
        )
        .await
        .expect("Should insert NewState msg");
        insert_validator_message(
            &app.pool,
            &channel,
            &channel.follower,
            &MessageTypes::ApproveState(approve_state),
        )
        .await
        .expect("Should insert NewState msg");

        // hardcoded proofs
        let spender_proof =
            "8ea7760ca2dbbe00673372afbf8b05048717ce8a305f1f853afac8c244182e0c".to_string();
        let earner_proof =
            "dc94141cb41550df047ba3a965ce36d98eb6098eb952ca3cb6fd9682e5810b51".to_string();

        // call functions
        let spender_leaf = get_leaf(
            app.clone(),
            channel_context.clone(),
            Extension(LeafFor::Spender),
            Path((channel.id(), *ADVERTISER)),
        )
        .await
        .expect("should get spender leaf");
        let earner_leaf = get_leaf(
            app.clone(),
            channel_context.clone(),
            Extension(LeafFor::Earner),
            Path((channel.id(), *PUBLISHER)),
        )
        .await
        .expect("should get earner leaf");

        // compare results
        assert_eq!(spender_proof, spender_leaf.merkle_proof);
        assert_eq!(earner_proof, earner_leaf.merkle_proof);
    }
}
