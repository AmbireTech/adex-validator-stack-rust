use std::{collections::HashMap, fmt};

use adapter::{prelude::*, Error as AdapterError};
use primitives::{
    balances,
    balances::{Balances, CheckedState, UncheckedState},
    spender::Spender,
    validator::{ApproveState, MessageTypes, NewState, RejectState},
    Address, ChainOf, Channel, UnifiedNum,
};

use crate::{
    core::follower_rules::{get_health, is_valid_transition},
    heartbeat::{heartbeat, HeartbeatStatus},
    sentry_interface::{Error as SentryApiError, PropagationResult, SentryApi},
    GetStateRoot, GetStateRootError,
};
use chrono::Utc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("overflow placeholder")]
    Overflow,
    #[error("The Channel's Token is not whitelisted")]
    TokenNotWhitelisted,
    #[error("Couldn't get state root hash of the proposed balances")]
    StateRootHash(#[from] GetStateRootError),
    #[error("Adapter error: {0}")]
    Adapter(#[from] AdapterError),
    #[error("Sentry API: {0}")]
    SentryApi(#[from] SentryApiError),
    #[error("Heartbeat: {0}")]
    Heartbeat(#[from] crate::heartbeat::Error),
}

#[derive(Debug)]
pub enum InvalidNewState {
    RootHash,
    Signature,
    Transition,
    Health(Health),
}

#[derive(Debug)]
pub enum Health {
    Earners(u64),
    Spenders(u64),
}

impl fmt::Display for InvalidNewState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let string = match self {
            InvalidNewState::RootHash => "InvalidRootHash",
            InvalidNewState::Signature => "InvalidSignature",
            InvalidNewState::Transition => "InvalidTransition",
            // TODO: Should we use health value?
            InvalidNewState::Health(health) => match health {
                Health::Earners(_health) => "TooLowHealthEarners",
                Health::Spenders(_health) => "TooLowHealthSpenders",
            },
        };

        write!(f, "{}", string)
    }
}

#[derive(Debug)]
pub enum ApproveStateResult {
    /// When `None` the conditions for approving the `NewState` (and generating `ApproveState`) haven't been met
    Sent(Option<Vec<PropagationResult>>),
    RejectedState {
        reason: InvalidNewState,
        state_root: String,
        propagation: Vec<PropagationResult>,
    },
}

#[derive(Debug)]
pub struct TickStatus {
    pub heartbeat: HeartbeatStatus,
    pub approve_state: ApproveStateResult,
}

pub async fn tick<C: Unlocked + 'static>(
    sentry: &SentryApi<C>,
    channel_context: &ChainOf<Channel>,
    all_spenders: HashMap<Address, Spender>,
    accounting_balances: Balances<CheckedState>,
) -> Result<TickStatus, Error> {
    let from = channel_context.context.leader;
    let channel_id = channel_context.context.id();

    // TODO: Context for All spender sum Error when overflow occurs
    let all_spenders_sum = all_spenders
        .values()
        .map(|spender| &spender.total_deposited)
        .sum::<Option<_>>()
        .ok_or(Error::Overflow)?;

    // if we don't have a `NewState` return `None`
    let new_msg = sentry
        .get_latest_msg(channel_id, from, &["NewState"])
        .await?
        .map(NewState::try_from)
        .transpose()
        .expect("Should always return a NewState message");

    let our_latest_msg_response = sentry
        .get_our_latest_msg(channel_id, &["ApproveState", "RejectState"])
        .await?;

    let our_latest_msg_state_root = match our_latest_msg_response {
        Some(MessageTypes::ApproveState(approve_state)) => Some(approve_state.state_root),
        Some(MessageTypes::RejectState(reject_state)) => Some(reject_state.state_root),
        _ => None,
    };

    let latest_is_responded_to = match (&new_msg, &our_latest_msg_state_root) {
        (Some(new_msg), Some(state_root)) => &new_msg.state_root == state_root,
        _ => false,
    };

    let approve_state_result = if let (Some(new_state), false) = (new_msg, latest_is_responded_to) {
        on_new_state(
            sentry,
            channel_context,
            accounting_balances,
            new_state,
            all_spenders_sum,
        )
        .await?
    } else {
        ApproveStateResult::Sent(None)
    };

    Ok(TickStatus {
        heartbeat: heartbeat(sentry, channel_context).await?,
        approve_state: approve_state_result,
    })
}

async fn on_new_state<'a, C: Unlocked + 'static>(
    sentry: &'a SentryApi<C>,
    channel_context: &'a ChainOf<Channel>,
    accounting_balances: Balances<CheckedState>,
    new_state: NewState<UncheckedState>,
    all_spenders_sum: UnifiedNum,
) -> Result<ApproveStateResult, Error> {
    let channel = channel_context.context;

    let proposed_balances = match new_state.balances.clone().check() {
        Ok(balances) => balances,
        // TODO: Should we show the Payout Mismatch between Spent & Earned?
        Err(balances::Error::PayoutMismatch { .. }) => {
            return on_error(
                sentry,
                channel_context,
                new_state,
                InvalidNewState::Transition,
            )
            .await;
        }
        // TODO: Add context for `proposed_balances.check()` overflow error
        Err(_) => return Err(Error::Overflow),
    };

    let proposed_state_root = new_state.state_root.clone();

    if proposed_state_root
        != proposed_balances.encode(channel.id(), channel_context.token.precision.get())?
    {
        return on_error(
            sentry,
            channel_context,
            new_state,
            InvalidNewState::RootHash,
        )
        .await;
    }

    if !sentry
        .adapter
        .verify(channel.leader, &proposed_state_root, &new_state.signature)?
    {
        return on_error(
            sentry,
            channel_context,
            new_state,
            InvalidNewState::Signature,
        )
        .await;
    }

    let last_approve_response = sentry.get_last_approved(channel.id()).await?;
    let prev_balances = match last_approve_response
        .last_approved
        .and_then(|last_approved| last_approved.new_state)
        .map(|new_state| new_state.msg.into_inner().balances.check())
        .transpose()
    {
        Ok(Some(previous_balances)) => previous_balances,
        Ok(None) => Default::default(),
        // TODO: Add Context for Transition error
        Err(_err) => {
            return on_error(
                sentry,
                channel_context,
                new_state,
                InvalidNewState::Transition,
            )
            .await;
        }
    };

    // OUTPACE rules:
    // 1. Check the transition of previous and proposed Spenders maps:
    //
    // sum(accounting.balances.spenders) > sum(new_state.balances.spenders)
    // & Each spender value in `next` should be > the corresponding `prev` value
    if !is_valid_transition(
        all_spenders_sum,
        &prev_balances.spenders,
        &proposed_balances.spenders,
    )
    .ok_or(Error::Overflow)?
    {
        // TODO: Add context for error in Spenders transition
        return on_error(
            sentry,
            channel_context,
            new_state,
            InvalidNewState::Transition,
        )
        .await;
    }

    // 2. Check the transition of previous and proposed Earners maps
    //
    // sum(accounting.balances.earners) > sum(new_state.balances.earners)
    // & Each spender value in `next` should be > the corresponding `prev` value
    // sum(accounting.balances.spenders) > sum(new_state.balances.spenders)
    if !is_valid_transition(
        all_spenders_sum,
        &prev_balances.earners,
        &proposed_balances.earners,
    )
    .ok_or(Error::Overflow)?
    {
        // TODO: Add context for error in Earners transition
        return on_error(
            sentry,
            channel_context,
            new_state,
            InvalidNewState::Transition,
        )
        .await;
    }

    let health_earners = get_health(
        all_spenders_sum,
        &accounting_balances.earners,
        &proposed_balances.earners,
    )
    .ok_or(Error::Overflow)?;
    if health_earners < u64::from(sentry.config.health_unsignable_promilles) {
        return on_error(
            sentry,
            channel_context,
            new_state,
            InvalidNewState::Health(Health::Earners(health_earners)),
        )
        .await;
    }

    let health_spenders = get_health(
        all_spenders_sum,
        &accounting_balances.spenders,
        &proposed_balances.spenders,
    )
    .ok_or(Error::Overflow)?;
    if health_spenders < u64::from(sentry.config.health_unsignable_promilles) {
        return on_error(
            sentry,
            channel_context,
            new_state,
            InvalidNewState::Health(Health::Spenders(health_spenders)),
        )
        .await;
    }

    let signature = sentry.adapter.sign(&new_state.state_root)?;
    let health_threshold = u64::from(sentry.config.health_threshold_promilles);
    let is_healthy = health_earners >= health_threshold && health_spenders >= health_threshold;

    let propagation_result = sentry
        .propagate(
            channel_context,
            &[MessageTypes::ApproveState(ApproveState {
                state_root: proposed_state_root,
                signature,
                is_healthy,
            })],
        )
        .await?;

    Ok(ApproveStateResult::Sent(Some(propagation_result)))
}

async fn on_error<'a, C: Unlocked + 'static>(
    sentry: &'a SentryApi<C>,
    channel_context: &ChainOf<Channel>,
    new_state: NewState<UncheckedState>,
    status: InvalidNewState,
) -> Result<ApproveStateResult, Error> {
    let propagation = sentry
        .propagate(
            channel_context,
            &[MessageTypes::RejectState(RejectState {
                reason: status.to_string(),
                state_root: new_state.state_root.clone(),
                signature: new_state.signature.clone(),
                balances: Some(new_state.balances.clone()),
                /// The timestamp when the NewState is being rejected
                timestamp: Some(Utc::now()),
            })],
        )
        .await?;

    Ok(ApproveStateResult::RejectedState {
        reason: status,
        state_root: new_state.state_root.clone(),
        propagation,
    })
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::sentry_interface::{AuthToken, ChainsValidators, Validator};
    use adapter::dummy::{Adapter, Dummy, Options};
    use chrono::Utc;
    use primitives::{
        balances::UncheckedState,
        config::{configuration, Environment},
        sentry::{
            message::Message, LastApproved, LastApprovedResponse, MessageResponse, SuccessResponse,
            ValidatorMessage, ValidatorMessagesListResponse,
        },
        test_util::{
            discard_logger, ADVERTISER, DUMMY_CAMPAIGN, DUMMY_VALIDATOR_FOLLOWER,
            DUMMY_VALIDATOR_LEADER, FOLLOWER, GUARDIAN, GUARDIAN_2, IDS, LEADER, PUBLISHER,
            PUBLISHER_2,
        },
        util::ApiUrl,
        validator::messages::{Heartbeat, NewState},
        ChainId, Config, ToETHChecksum, UnifiedNum, ValidatorId,
    };
    use std::{collections::HashMap, str::FromStr};
    use wiremock::{
        matchers::{method, path, query_param},
        Mock, MockServer, ResponseTemplate,
    };

    async fn setup_mock_server() -> MockServer {
        // Set up wiremock to return success:true when propagating to both leader and follower
        let server = MockServer::start().await;
        let ok_response = SuccessResponse { success: true };
        Mock::given(method("POST"))
            .and(path(format!(
                "leader/v5/channel/{}/validator-messages",
                DUMMY_CAMPAIGN.channel.id()
            )))
            .respond_with(ResponseTemplate::new(200).set_body_json(&ok_response))
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path(format!(
                "follower/v5/channel/{}/validator-messages",
                DUMMY_CAMPAIGN.channel.id()
            )))
            .respond_with(ResponseTemplate::new(200).set_body_json(&ok_response))
            .mount(&server)
            .await;

        let heartbeat = Heartbeat {
            signature: String::new(),
            state_root: String::new(),
            timestamp: Utc::now(),
        };
        let heartbeat_res = ValidatorMessagesListResponse {
            messages: vec![ValidatorMessage {
                from: DUMMY_CAMPAIGN.channel.follower,
                received: Utc::now(),
                msg: MessageTypes::Heartbeat(heartbeat),
            }],
        };
        Mock::given(method("GET"))
            .and(path(format!(
                "/v5/channel/{}/validator-messages/{}/{}",
                DUMMY_CAMPAIGN.channel.id(),
                DUMMY_CAMPAIGN.channel.leader,
                "Heartbeat",
            )))
            .and(query_param("limit", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&heartbeat_res))
            .mount(&server)
            .await;
        server
    }

    async fn setup_sentry(server: &MockServer, config: &Config) -> SentryApi<Dummy> {
        let sentry_url = ApiUrl::from_str(&server.uri()).expect("Should parse");

        let adapter = Adapter::with_unlocked(Dummy::init(Options {
            dummy_identity: IDS[&LEADER],
            dummy_auth_tokens: vec![(IDS[&LEADER].to_address(), "AUTH_Leader".into())]
                .into_iter()
                .collect(),
        }));
        let logger = discard_logger();

        let mut validators: HashMap<ValidatorId, Validator> = HashMap::new();
        let leader = Validator {
            url: ApiUrl::from_str(&format!("{}/leader", server.uri())).expect("should be valid"),
            token: AuthToken::default(),
        };
        let follower = Validator {
            url: ApiUrl::from_str(&format!("{}/follower", server.uri())).expect("should be valid"),
            token: AuthToken::default(),
        };
        validators.insert(DUMMY_VALIDATOR_LEADER.id, leader);
        validators.insert(DUMMY_VALIDATOR_FOLLOWER.id, follower);
        let mut propagate_to: ChainsValidators = HashMap::new();
        propagate_to.insert(ChainId::from(1337), validators);

        SentryApi::new(adapter, logger, config.clone(), sentry_url)
            .expect("Should create instance")
            .with_propagate(propagate_to)
            .expect("Should propagate")
    }

    async fn setup_new_state_response(
        server: &MockServer,
        new_state_msg: Option<NewState<UncheckedState>>,
    ) {
        let new_state_res = match new_state_msg {
            Some(msg) => ValidatorMessagesListResponse {
                messages: vec![ValidatorMessage {
                    from: DUMMY_CAMPAIGN.channel.leader,
                    received: Utc::now(),
                    msg: MessageTypes::NewState(msg),
                }],
            },
            None => ValidatorMessagesListResponse { messages: vec![] },
        };

        Mock::given(method("GET"))
            .and(path(format!(
                "/v5/channel/{}/validator-messages/{}/{}",
                DUMMY_CAMPAIGN.channel.id(),
                DUMMY_CAMPAIGN.channel.leader,
                "NewState",
            )))
            .and(query_param("limit", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&new_state_res))
            .mount(&server)
            .await;
    }

    async fn setup_approve_state_response(
        server: &MockServer,
        approve_state: Option<ApproveState>,
    ) {
        let approve_state_res = match approve_state {
            Some(msg) => ValidatorMessagesListResponse {
                messages: vec![ValidatorMessage {
                    from: DUMMY_CAMPAIGN.channel.follower,
                    received: Utc::now(),
                    msg: MessageTypes::ApproveState(msg),
                }],
            },
            None => ValidatorMessagesListResponse { messages: vec![] },
        };

        Mock::given(method("GET"))
            .and(path(format!(
                "/v5/channel/{}/validator-messages/{}/{}",
                DUMMY_CAMPAIGN.channel.id(),
                DUMMY_CAMPAIGN.channel.follower,
                "ApproveState+RejectState",
            )))
            .and(query_param("limit", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&approve_state_res))
            .mount(&server)
            .await;
    }

    async fn setup_reject_state_response(
        server: &MockServer,
        reject_state: Option<RejectState<UncheckedState>>,
    ) {
        let reject_state_res = match reject_state {
            Some(msg) => ValidatorMessagesListResponse {
                messages: vec![ValidatorMessage {
                    from: DUMMY_CAMPAIGN.channel.follower,
                    received: Utc::now(),
                    msg: MessageTypes::RejectState(msg),
                }],
            },
            None => ValidatorMessagesListResponse { messages: vec![] },
        };

        Mock::given(method("GET"))
            .and(path(format!(
                "/v5/channel/{}/validator-messages/{}/{}",
                DUMMY_CAMPAIGN.channel.id(),
                DUMMY_CAMPAIGN.channel.follower,
                "ApproveState+RejectState",
            )))
            .and(query_param("limit", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&reject_state_res))
            .mount(&server)
            .await;
    }

    async fn setup_last_approved_response(
        server: &MockServer,
        balances: Balances<UncheckedState>,
        channel_context: &ChainOf<Channel>,
    ) {
        // In the case of a payout mismatch, the value of the state_root won't matter
        let state_root = match balances.clone().check() {
            Ok(balances) => balances
                .encode(
                    channel_context.context.id(),
                    channel_context.token.precision.get(),
                )
                .expect("should encode"),
            Err(_) => String::new(),
        };
        let last_approved_new_state: NewState<UncheckedState> = NewState {
            state_root,
            signature: IDS[&*LEADER].to_checksum(),
            balances: balances.into_unchecked(),
        };
        let new_state_res = MessageResponse {
            from: IDS[&*LEADER],
            received: Utc::now(),
            msg: Message::new(last_approved_new_state),
        };
        let last_approved_response = LastApprovedResponse {
            last_approved: Some(LastApproved {
                new_state: Some(new_state_res),
                approve_state: None,
            }),
            heartbeats: None,
        };

        Mock::given(method("GET"))
            .and(path(format!(
                "/v5/channel/{}/last-approved",
                DUMMY_CAMPAIGN.channel.id(),
            )))
            .and(query_param("withHeartbeat", "true"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&last_approved_response))
            .mount(&server)
            .await;
    }
    #[tokio::test]
    async fn test_follower_tick() {
        let server = setup_mock_server().await;
        let config = configuration(Environment::Development, None).expect("Should get Config");
        let sentry = setup_sentry(&server, &config).await;

        let channel_context = config
            .find_chain_of(DUMMY_CAMPAIGN.channel.token)
            .expect("Should find Dummy campaign token in config")
            .with_channel(DUMMY_CAMPAIGN.channel);

        let get_initial_balances = || {
            let mut balances: Balances<CheckedState> = Balances::new();
            balances
                .spend(*ADVERTISER, *PUBLISHER, UnifiedNum::from_u64(1000))
                .expect("should spend");
            balances
                .spend(*ADVERTISER, *PUBLISHER_2, UnifiedNum::from_u64(1000))
                .expect("should spend");
            balances
                .spend(*GUARDIAN, *PUBLISHER, UnifiedNum::from_u64(1000))
                .expect("should spend");
            balances
                .spend(*GUARDIAN, *PUBLISHER_2, UnifiedNum::from_u64(1000))
                .expect("should spend");
            balances
        };

        let get_initial_spenders = || {
            let mut spenders: HashMap<Address, Spender> = HashMap::new();
            spenders.insert(
                *ADVERTISER,
                Spender {
                    total_deposited: UnifiedNum::from_u64(10_000),
                    total_spent: Some(UnifiedNum::from_u64(2000)),
                },
            );
            spenders.insert(
                *GUARDIAN,
                Spender {
                    total_deposited: UnifiedNum::from_u64(10_000),
                    total_spent: Some(UnifiedNum::from_u64(2000)),
                },
            );
            spenders
        };

        // Case where all_spenders_sum overflows
        {
            let mut all_spenders: HashMap<Address, Spender> = HashMap::new();

            all_spenders.insert(
                *ADVERTISER,
                Spender {
                    total_deposited: UnifiedNum::from_u64(u64::MAX),
                    total_spent: None,
                },
            );
            all_spenders.insert(
                *GUARDIAN,
                Spender {
                    total_deposited: UnifiedNum::from_u64(u64::MAX),
                    total_spent: None,
                },
            );

            let tick_res = tick(
                &sentry,
                &channel_context,
                all_spenders,
                get_initial_balances(),
            )
            .await;
            assert!(matches!(tick_res, Err(Error::Overflow)));
        }

        // - Payout mismatch when checking new_state.balances()
        {
            let mut new_state_balances = get_initial_balances().into_unchecked();
            new_state_balances
                .earners
                .insert(*GUARDIAN_2, UnifiedNum::from_u64(10000));
            let new_state: NewState<UncheckedState> = NewState {
                state_root: String::new(),
                signature: IDS[&*LEADER].to_checksum(),
                balances: new_state_balances,
            };
            let res = on_new_state(
                &sentry,
                &channel_context,
                get_initial_balances(),
                new_state,
                UnifiedNum::from_u64(4000),
            )
            .await
            .expect("Shouldn't return an error");
            assert!(matches!(
                res,
                ApproveStateResult::RejectedState {
                    reason: InvalidNewState::Transition,
                    ..
                }
            ));
        }
        // - Case where new_state.state_root won’t be the same as proposed_balances.encode(…) -> proposed balances are different
        {
            let mut new_state_balances = get_initial_balances();
            new_state_balances
                .spend(*PUBLISHER, *GUARDIAN, UnifiedNum::from_u64(10000))
                .expect("should spend");
            let state_root = new_state_balances
                .encode(
                    channel_context.context.id(),
                    channel_context.token.precision.get(),
                )
                .expect("should encode");
            let new_state: NewState<UncheckedState> = NewState {
                state_root,
                signature: IDS[&*LEADER].to_checksum(),
                balances: new_state_balances.into_unchecked(),
            };
            let res = on_new_state(
                &sentry,
                &channel_context,
                get_initial_balances(),
                new_state,
                UnifiedNum::from_u64(4000),
            )
            .await
            .expect("Shouldn't return an error");
            assert!(matches!(
                res,
                ApproveStateResult::RejectedState {
                    reason: InvalidNewState::RootHash,
                    ..
                }
            ));
        }

        // - Case where sentry.adapter.verify(…) will return false -> signature is from a different validator
        {
            let proposed_balances = get_initial_balances();
            let state_root = proposed_balances
                .encode(
                    channel_context.context.id(),
                    channel_context.token.precision.get(),
                )
                .expect("should encode");
            let new_state: NewState<UncheckedState> = NewState {
                state_root: state_root,
                signature: IDS[&*FOLLOWER].to_checksum(),
                balances: proposed_balances.into_unchecked(),
            };
            let res = on_new_state(
                &sentry,
                &channel_context,
                get_initial_balances(),
                new_state,
                UnifiedNum::from_u64(4000),
            )
            .await
            .expect("Shouldn't return an error");
            assert!(matches!(
                res,
                ApproveStateResult::RejectedState {
                    reason: InvalidNewState::Signature,
                    ..
                }
            ));
        }

        // - Case where LastApprovedResponse new state balances have a payout mismatch resulting in an error
        {
            let mut last_approved_balances = get_initial_balances().into_unchecked();
            last_approved_balances
                .earners
                .insert(*GUARDIAN_2, UnifiedNum::from_u64(10_000));
            setup_last_approved_response(&server, last_approved_balances, &channel_context).await;

            let proposed_balances = get_initial_balances();
            let state_root = proposed_balances
                .encode(
                    channel_context.context.id(),
                    channel_context.token.precision.get(),
                )
                .expect("should encode");
            let new_state: NewState<UncheckedState> = NewState {
                state_root: state_root,
                signature: IDS[&*LEADER].to_checksum(),
                balances: proposed_balances.into_unchecked(),
            };
            let res = on_new_state(
                &sentry,
                &channel_context,
                get_initial_balances(),
                new_state,
                UnifiedNum::from_u64(4000),
            )
            .await
            .expect("Shouldn't return an error");
            assert!(matches!(
                res,
                ApproveStateResult::RejectedState {
                    reason: InvalidNewState::Signature,
                    ..
                }
            ));
        }

        // - Case where is_valid_transition() fails (proposed balances < previous balances)
        {
            let mut last_approved_balances = get_initial_balances();
            last_approved_balances
                .spend(*ADVERTISER, *PUBLISHER, UnifiedNum::from_u64(2000))
                .expect("should spend");
            setup_last_approved_response(
                &server,
                last_approved_balances.into_unchecked(),
                &channel_context,
            )
            .await;

            let proposed_balances = get_initial_balances();
            let state_root = proposed_balances
                .encode(
                    channel_context.context.id(),
                    channel_context.token.precision.get(),
                )
                .expect("should encode");
            let new_state: NewState<UncheckedState> = NewState {
                state_root: state_root,
                signature: IDS[&*LEADER].to_checksum(),
                balances: proposed_balances.into_unchecked(),
            };
            let res = on_new_state(
                &sentry,
                &channel_context,
                get_initial_balances(),
                new_state,
                UnifiedNum::from_u64(4000),
            )
            .await
            .expect("Shouldn't return an error");
            assert!(matches!(
                res,
                ApproveStateResult::RejectedState {
                    reason: InvalidNewState::Transition,
                    ..
                }
            ));
        }

        // - Case where get_health() will return less than 750 promilles
        {
            setup_last_approved_response(
                &server,
                get_initial_balances().into_unchecked(),
                &channel_context,
            )
            .await;

            let mut our_balances = get_initial_balances();
            our_balances
                .spend(*ADVERTISER, *PUBLISHER, UnifiedNum::from_u64(200_000))
                .expect("should spend");
            our_balances
                .spend(*GUARDIAN, *PUBLISHER_2, UnifiedNum::from_u64(200_000))
                .expect("should spend");

            let state_root = get_initial_balances()
                .encode(
                    channel_context.context.id(),
                    channel_context.token.precision.get(),
                )
                .expect("should encode");
            let new_state: NewState<UncheckedState> = NewState {
                state_root: state_root,
                signature: IDS[&*LEADER].to_checksum(),
                balances: get_initial_balances().into_unchecked(),
            };
            let res = on_new_state(
                &sentry,
                &channel_context,
                our_balances,
                new_state,
                UnifiedNum::from_u64(1_000_000),
            )
            .await
            .expect("Shouldn't return an error");
            assert!(matches!(
                res,
                ApproveStateResult::RejectedState {
                    reason: InvalidNewState::Health(..),
                    ..
                }
            ));
        }

        // Case where no NewState is returned
        {
            // Setting up the expected response
            setup_new_state_response(&server, None).await;
            setup_approve_state_response(&server, None).await;

            let tick_status = tick(
                &sentry,
                &channel_context,
                get_initial_spenders(),
                get_initial_balances(),
            )
            .await
            .expect("Shouldn't return an error");
            assert!(matches!(
                tick_status.approve_state,
                ApproveStateResult::Sent(None)
            ));
        }

        // Case where the NewState/ApproveState pair has matching state roots resulting in ApproveStateResult::Sent(None)
        {
            // Setting up the expected responses
            let state_root = get_initial_balances()
                .encode(
                    channel_context.context.id(),
                    channel_context.token.precision.get(),
                )
                .expect("should encode");
            let new_state: NewState<UncheckedState> = NewState {
                state_root: state_root.clone(),
                signature: IDS[&*LEADER].to_checksum(),
                balances: get_initial_balances().into_unchecked(),
            };
            setup_new_state_response(&server, Some(new_state)).await;
            let approve_state = ApproveState {
                state_root,
                signature: IDS[&*FOLLOWER].to_checksum(),
                is_healthy: true,
            };
            setup_approve_state_response(&server, Some(approve_state)).await;

            let tick_status = tick(
                &sentry,
                &channel_context,
                get_initial_spenders(),
                get_initial_balances(),
            )
            .await
            .expect("Shouldn't return an error");
            assert!(matches!(
                tick_status.approve_state,
                ApproveStateResult::Sent(None)
            ));
        }

        // Case where the NewState/RejectState pair has matching state roots resulting in ApproveStateResult::Sent(None)
        {
            let received = Utc::now();
            // Setting up the expected responses
            let state_root = get_initial_balances()
                .encode(
                    channel_context.context.id(),
                    channel_context.token.precision.get(),
                )
                .expect("should encode");
            let new_state: NewState<UncheckedState> = NewState {
                state_root: state_root.clone(),
                signature: IDS[&*LEADER].to_checksum(),
                balances: get_initial_balances().into_unchecked(),
            };
            setup_new_state_response(&server, Some(new_state)).await;

            let reject_state = RejectState {
                state_root,
                signature: IDS[&*FOLLOWER].to_checksum(),
                timestamp: Some(received),
                reason: "rejected".to_string(),
                balances: None,
            };
            setup_reject_state_response(&server, Some(reject_state)).await;
            let tick_status = tick(
                &sentry,
                &channel_context,
                get_initial_spenders(),
                get_initial_balances(),
            )
            .await
            .expect("Shouldn't return an error");
            assert!(matches!(
                tick_status.approve_state,
                ApproveStateResult::Sent(None)
            ));
        }

        // - Case where output will be ApproveStateResult::Sent(Some(propagation_result)) (all rules have been met)
        {
            setup_last_approved_response(
                &server,
                get_initial_balances().into_unchecked(),
                &channel_context,
            )
            .await;

            let state_root = get_initial_balances()
                .encode(
                    channel_context.context.id(),
                    channel_context.token.precision.get(),
                )
                .expect("should encode");
            let new_state: NewState<UncheckedState> = NewState {
                state_root: state_root,
                signature: IDS[&*LEADER].to_checksum(),
                balances: get_initial_balances().into_unchecked(),
            };
            let res = on_new_state(
                &sentry,
                &channel_context,
                get_initial_balances(),
                new_state,
                UnifiedNum::from_u64(1_000_000),
            )
            .await
            .expect("Shouldn't return an error");
            assert!(matches!(res, ApproveStateResult::Sent(Some(..))));
        }
    }
}
