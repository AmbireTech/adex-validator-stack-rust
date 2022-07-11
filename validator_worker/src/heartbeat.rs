use chrono::Utc;

use adapter::{prelude::*, util::get_signable_state_root, Error as AdapterError};
use byteorder::{BigEndian, ByteOrder};
use primitives::{
    merkle_tree::MerkleTree,
    validator::{Heartbeat, MessageTypes},
    ChainOf, Channel,
};
use thiserror::Error;

use crate::sentry_interface::{Error as SentryApiError, PropagationResult, SentryApi};

pub type HeartbeatStatus = Option<Vec<PropagationResult>>;

#[derive(Debug, Error)]
pub enum Error {
    #[error("MerkleTree: {0}")]
    MerkleTree(#[from] primitives::merkle_tree::Error),
    #[error("Adapter error: {0}")]
    Adapter(#[from] AdapterError),
    #[error("Sentry API: {0}")]
    SentryApi(#[from] SentryApiError),
}

pub async fn heartbeat<C: Unlocked + 'static>(
    iface: &SentryApi<C>,
    channel_context: &ChainOf<Channel>,
) -> Result<HeartbeatStatus, Error> {
    let validator_message_response = iface
        .get_our_latest_msg(channel_context.context.id(), &["Heartbeat"])
        .await?;
    let heartbeat_msg = match validator_message_response {
        Some(MessageTypes::Heartbeat(heartbeat)) => Some(heartbeat),
        _ => None,
    };

    let should_send = heartbeat_msg.map_or(true, |heartbeat| {
        let duration = (Utc::now() - heartbeat.timestamp)
            .to_std()
            .expect("Should never panic because Now > heartbeat.timestamp");

        duration > iface.config.heartbeat_time
    });

    if should_send {
        Ok(Some(send_heartbeat(iface, channel_context).await?))
    } else {
        Ok(None)
    }
}

async fn send_heartbeat<C: Unlocked + 'static>(
    iface: &SentryApi<C>,
    channel_context: &ChainOf<Channel>,
) -> Result<Vec<PropagationResult>, Error> {
    let timestamp = Utc::now();
    let mut timestamp_buf = [0_u8; 32];
    let milliseconds: u64 = u64::try_from(timestamp.timestamp_millis())
        .expect("The timestamp should be able to be converted to u64");
    BigEndian::write_uint(&mut timestamp_buf[26..], milliseconds, 6);

    let merkle_tree = MerkleTree::new(&[timestamp_buf])?;

    let state_root_raw =
        get_signable_state_root(channel_context.context.id().as_ref(), &merkle_tree.root());
    let state_root = hex::encode(state_root_raw);

    let signature = iface.adapter.sign(&state_root)?;

    let message_types = MessageTypes::Heartbeat(Heartbeat {
        signature,
        state_root,
        timestamp,
    });

    Ok(iface.propagate(channel_context, &[message_types]).await?)
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::sentry_interface::{ChainsValidators, Validator};
    use adapter::dummy::{Adapter, Dummy, Options};
    use chrono::{Duration, Utc};
    use primitives::{
        config::GANACHE_CONFIG,
        sentry::{SuccessResponse, ValidatorMessage, ValidatorMessagesListResponse},
        test_util::{
            discard_logger, DUMMY_AUTH, DUMMY_CAMPAIGN, DUMMY_VALIDATOR_FOLLOWER,
            DUMMY_VALIDATOR_LEADER, FOLLOWER, IDS, LEADER,
        },
        util::ApiUrl,
        validator::messages::Heartbeat,
        ChainId, Config, ValidatorId,
    };
    use std::{collections::HashMap, str::FromStr};
    use wiremock::{
        matchers::{method, path, query_param},
        Mock, MockGuard, MockServer, ResponseTemplate,
    };

    // Sets up wiremock server instance and responses which are shared for all test cases
    async fn setup_mock_server() -> MockServer {
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

        server
    }

    async fn setup_sentry(server: &MockServer, config: &Config) -> SentryApi<Dummy> {
        let sentry_url = ApiUrl::from_str(&server.uri()).expect("Should parse");

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

    async fn setup_heartbeat_res(server: &MockServer, heartbeat: Heartbeat) -> MockGuard {
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
            .expect(1)
            .mount_as_scoped(server)
            .await
    }

    #[tokio::test]
    async fn test_heartbeats() {
        let config = GANACHE_CONFIG.clone();
        let server = setup_mock_server().await;
        let sentry = setup_sentry(&server, &config).await;
        {
            let heartbeat_msg = Heartbeat {
                signature: String::new(),
                state_root: String::new(),
                timestamp: Utc::now(),
            };
            let _mock_guard = setup_heartbeat_res(&server, heartbeat_msg).await;

            let channel_context = config
                .find_chain_of(DUMMY_CAMPAIGN.channel.token)
                .expect("Should find Dummy campaign token in config")
                .with_channel(DUMMY_CAMPAIGN.channel);

            let res = heartbeat(&sentry, &channel_context)
                .await
                .expect("shouldn't return an error");

            assert!(res.is_none());
        }

        // Old heartbeat
        {
            // Using sleep(config.heartbeat_time) would make our test freeze for 30 seconds so just modifying the timestamp is more efficient
            let heartbeat_msg = Heartbeat {
                signature: String::new(),
                state_root: String::new(),
                timestamp: Utc::now() - Duration::minutes(10),
            };
            let _mock_guard = setup_heartbeat_res(&server, heartbeat_msg).await;

            let channel_context = config
                .find_chain_of(DUMMY_CAMPAIGN.channel.token)
                .expect("Should find Dummy campaign token in config")
                .with_channel(DUMMY_CAMPAIGN.channel);

            let res = heartbeat(&sentry, &channel_context)
                .await
                .expect("shouldn't return an error");

            assert!(res.is_some());

            let propagated_to: Vec<ValidatorId> = res
                .unwrap()
                .into_iter()
                .collect::<Result<Vec<_>, _>>()
                .expect("Shouldn't return an error");
            assert!(
                propagated_to.contains(&IDS[&*LEADER]),
                "Heartbeat message is propagated to the leader validator"
            );
            assert!(
                propagated_to.contains(&IDS[&*FOLLOWER]),
                "Heartbeat message is propagated to the follower validator"
            );
            assert_eq!(
                propagated_to.len(),
                2,
                "Heartbeat message isn't propagated to any other validator"
            );
        }
    }
}
