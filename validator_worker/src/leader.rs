use thiserror::Error;

use adapter::{prelude::*, Error as AdapterError};
use primitives::{
    balances::CheckedState,
    validator::{MessageError, MessageTypes, NewState},
    Balances, ChainOf, Channel,
};

use crate::{
    heartbeat::{heartbeat, Error as HeartbeatError, HeartbeatStatus},
    sentry_interface::{Error as SentryApiError, PropagationResult, SentryApi},
    GetStateRoot, GetStateRootError,
};

#[derive(Debug)]
pub struct TickStatus {
    pub heartbeat: HeartbeatStatus,
    /// When `None` the conditions for creating a `NewState` haven't been met
    pub new_state: Option<Vec<PropagationResult>>,
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("SentryApi: {0}")]
    SentryApi(#[from] SentryApiError),
    #[error("StateRootHash: {0}")]
    StateRootHash(#[from] GetStateRootError),
    #[error("Adapter: {0}")]
    Adapter(#[from] AdapterError),
    #[error("Heartbeat: {0}")]
    Heartbeat(#[from] HeartbeatError),
    #[error("NewState Balances: {0}")]
    Message(#[from] MessageError<NewState<CheckedState>>),
    #[error("Overflow")]
    Overflow,
}

pub async fn tick<C: Unlocked + 'static>(
    sentry: &SentryApi<C>,
    channel_context: &ChainOf<Channel>,
    accounting_balances: Balances<CheckedState>,
) -> Result<TickStatus, Error> {
    let channel = channel_context.context;

    // Check if Accounting != than latest NewState (Accounting.balances != NewState.balances)
    let should_generate_new_state =
        {
            // If the accounting is empty, then we don't need to create a NewState
            if accounting_balances.earners.is_empty() || accounting_balances.spenders.is_empty() {
                false
            } else {
                let latest_new_state = sentry
                    .get_our_latest_msg(channel.id(), &["NewState"])
                    .await?
                    .map(NewState::<CheckedState>::try_from)
                    .transpose()?;

                match latest_new_state {
                    Some(new_state) => {
                        let check_spenders = accounting_balances.spenders.iter().any(
                            |(spender, accounting_balance)| {
                                match new_state.balances.spenders.get(spender) {
                                    Some(prev_balance) => accounting_balance > prev_balance,
                                    // if there is no previous balance for this Spender then it should generate a `NewState`
                                    // this includes adding an empty Spender to be included in the MerkleTree
                                    None => true,
                                }
                            },
                        );

                        let check_earners = accounting_balances.earners.iter().any(
                            |(earner, accounting_balance)| {
                                match new_state.balances.earners.get(earner) {
                                    Some(prev_balance) => accounting_balance > prev_balance,
                                    // if there is no previous balance for this Earner then it should generate a `NewState`
                                    // this includes adding an empty Earner to be included in the MerkleTree
                                    None => true,
                                }
                            },
                        );

                        check_spenders || check_earners
                    }
                    // if no previous `NewState` (i.e. `Channel` is new) - it should generate a `NewState`
                    // this is only valid if the Accounting balances are not empty!
                    None => true,
                }
            }
        };

    // Create a `NewState` if balances have changed
    let new_state = if should_generate_new_state {
        Some(on_new_accounting(sentry, channel_context, accounting_balances).await?)
    } else {
        None
    };

    Ok(TickStatus {
        heartbeat: heartbeat(sentry, channel_context).await?,
        new_state,
    })
}

async fn on_new_accounting<C: Unlocked + 'static>(
    sentry: &SentryApi<C>,
    channel_context: &ChainOf<Channel>,
    accounting_balances: Balances<CheckedState>,
) -> Result<Vec<PropagationResult>, Error> {
    let state_root = accounting_balances.encode(
        channel_context.context.id(),
        channel_context.token.precision.get(),
    )?;

    let signature = sentry.adapter.sign(&state_root)?;

    let propagation_results = sentry
        .propagate(
            channel_context,
            &[MessageTypes::NewState(NewState {
                state_root,
                signature,
                balances: accounting_balances.into_unchecked(),
            })],
        )
        .await?;

    Ok(propagation_results)
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::sentry_interface::{ChainsValidators, Validator};
    use adapter::dummy::{Adapter, Dummy, Options};
    use chrono::Utc;
    use primitives::{
        balances::UncheckedState,
        campaign::Validators,
        config::GANACHE_CONFIG,
        sentry::{SuccessResponse, ValidatorMessage, ValidatorMessagesListResponse},
        test_util::{
            discard_logger, ADVERTISER, CREATOR, DUMMY_AUTH, DUMMY_CAMPAIGN,
            DUMMY_VALIDATOR_FOLLOWER, DUMMY_VALIDATOR_LEADER, FOLLOWER, IDS, LEADER, PUBLISHER,
            PUBLISHER_2,
        },
        util::ApiUrl,
        validator::{Heartbeat, MessageTypes, NewState},
        ChainId, Config, ToETHChecksum, UnifiedNum, ValidatorDesc, ValidatorId,
    };
    use std::{collections::HashMap, str::FromStr};
    use wiremock::{
        matchers::{method, path, query_param},
        Mock, MockGuard, MockServer, ResponseTemplate,
    };
    // Initializes a SentryApi instance
    async fn setup_sentry(server: &MockServer, config: &Config) -> SentryApi<Dummy> {
        let sentry_url =
            ApiUrl::from_str(&format!("{}/leader", &server.uri())).expect("Should parse");

        let adapter = Adapter::with_unlocked(Dummy::init(Options {
            dummy_identity: IDS[&LEADER],
            dummy_auth_tokens: DUMMY_AUTH.clone(),
            dummy_chains: config.chains.values().cloned().collect(),
        }));
        let logger = discard_logger();

        let mut validators: HashMap<ValidatorId, Validator> = HashMap::new();
        let leader = Validator {
            url: ApiUrl::from_str(&format!("{}/leader", server.uri())).expect("should be valid"),
            token: DUMMY_AUTH
                .get(&*LEADER)
                .expect("should be valid")
                .to_string(),
        };
        let follower = Validator {
            url: ApiUrl::from_str(&format!("{}/follower", server.uri())).expect("should be valid"),
            token: DUMMY_AUTH
                .get(&*FOLLOWER)
                .expect("should be valid")
                .to_string(),
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
    ) -> MockGuard {
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
                "/leader/v5/channel/{}/validator-messages/{}/{}",
                DUMMY_CAMPAIGN.channel.id(),
                DUMMY_CAMPAIGN.channel.leader,
                "NewState",
            )))
            .and(query_param("limit", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&new_state_res))
            .expect(1)
            .named("GET NewState helper")
            .mount_as_scoped(server)
            .await
    }

    #[tokio::test]
    async fn test_leader_tick() {
        let config = GANACHE_CONFIG.clone();
        let server = MockServer::start().await;
        let ok_response = SuccessResponse { success: true };
        let mut mock_campaign = DUMMY_CAMPAIGN.clone();
        mock_campaign.validators = Validators::new((
            ValidatorDesc {
                id: IDS[&LEADER],
                url: format!("{}/leader", server.uri()),
                fee: 100.into(),
                fee_addr: None,
            },
            ValidatorDesc {
                id: IDS[&LEADER],
                url: format!("{}/follower", server.uri()),
                fee: 100.into(),
                fee_addr: None,
            },
        ));

        // Making sure all propagations to leader/follower succeed
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
                from: DUMMY_CAMPAIGN.channel.leader,
                received: Utc::now(),
                msg: MessageTypes::Heartbeat(heartbeat),
            }],
        };
        Mock::given(method("GET"))
            .and(path(format!(
                "/leader/v5/channel/{}/validator-messages/{}/{}",
                DUMMY_CAMPAIGN.channel.id(),
                DUMMY_CAMPAIGN.channel.leader,
                "Heartbeat",
            )))
            .and(query_param("limit", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&heartbeat_res))
            .mount(&server)
            .await;
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
                .spend(*CREATOR, *PUBLISHER, UnifiedNum::from_u64(1000))
                .expect("should spend");
            balances
                .spend(*CREATOR, *PUBLISHER_2, UnifiedNum::from_u64(1000))
                .expect("should spend");
            balances
        };

        // Test case for empty balances
        {
            let balances: Balances<CheckedState> = Balances::new();
            let tick_result = tick(&sentry, &channel_context, balances)
                .await
                .expect("Shouldn't return an error");
            assert!(
                tick_result.new_state.is_none(),
                "Shouldn't generate a NewState when balances are empty"
            );
        }
        // No NewState message is returned -> Channel has just been created
        {
            let _mock_guard = setup_new_state_response(&server, None).await;

            let tick_result = tick(&sentry, &channel_context, get_initial_balances())
                .await
                .expect("Shouldn't return an error");
            assert!(
                tick_result.new_state.is_some(),
                "A NewState message should be generated when there isn't one"
            );
            let propagated_to: Vec<ValidatorId> = tick_result
                .new_state
                .unwrap()
                .into_iter()
                .collect::<Result<Vec<_>, _>>()
                .expect("Shouldn't return an error");
            assert!(
                propagated_to.contains(&IDS[&*LEADER]),
                "NewState message is propagated to the leader validator"
            );
            assert!(
                propagated_to.contains(&IDS[&*FOLLOWER]),
                "NewState message is propagated to the follower validator"
            );
            assert_eq!(
                propagated_to.len(),
                2,
                "NewState message isn't propagated to any other validator"
            );
        }

        // Test case where both spender and earner balances in the returned NewState message are equal to the ones in accounting_balances thus no new_state will be generated
        {
            // Setting up the expected response
            let proposed_balances = get_initial_balances();
            let state_root = proposed_balances
                .encode(
                    channel_context.context.id(),
                    channel_context.token.precision.get(),
                )
                .expect("should encode");
            let new_state: NewState<UncheckedState> = NewState {
                state_root,
                signature: IDS[&*LEADER].to_checksum(),
                balances: proposed_balances.into_unchecked(),
            };
            let _mock_guard = setup_new_state_response(&server, Some(new_state)).await;

            let tick_result = tick(&sentry, &channel_context, get_initial_balances())
                .await
                .expect("Shouldn't return an error");
            assert!(tick_result.new_state.is_none(), "Shouldn't generate a NewState when the balances in the previous NewState are equal to the ones in accounting_balances");
        }

        // Balance returned from the NewState message is lower for an earner/spender than the one in accounting_balances
        {
            // Setting up the expected response
            let proposed_balances = get_initial_balances();
            let state_root = proposed_balances
                .encode(
                    channel_context.context.id(),
                    channel_context.token.precision.get(),
                )
                .expect("should encode");
            let new_state: NewState<UncheckedState> = NewState {
                state_root,
                signature: IDS[&*LEADER].to_checksum(),
                balances: proposed_balances.into_unchecked(),
            };
            let _mock_guard = setup_new_state_response(&server, Some(new_state)).await;

            let mut expected_balances = get_initial_balances();
            expected_balances
                .spend(*ADVERTISER, *PUBLISHER, UnifiedNum::from_u64(1000))
                .expect("should spend");
            let tick_result = tick(&sentry, &channel_context, expected_balances)
                .await
                .expect("Shouldn't return an error");
            assert!(
                tick_result.new_state.is_some(),
                "NewState message should be generated accounting_balances have been changed"
            );
            let propagated_to: Vec<ValidatorId> = tick_result
                .new_state
                .unwrap()
                .into_iter()
                .collect::<Result<Vec<_>, _>>()
                .expect("Shouldn't return an error");
            assert!(
                propagated_to.contains(&IDS[&*LEADER]),
                "NewState message is propagated to the leader validator"
            );
            assert!(
                propagated_to.contains(&IDS[&*FOLLOWER]),
                "NewState message is propagated to the follower validator"
            );
            assert_eq!(
                propagated_to.len(),
                2,
                "NewState message isn't propagated to any other validator"
            );
        }
    }
}
