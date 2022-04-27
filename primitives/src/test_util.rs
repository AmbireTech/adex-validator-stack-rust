use std::{collections::HashMap, ops::Deref};

use chrono::{TimeZone, Utc};
use once_cell::sync::Lazy;

use crate::{
    balances::{Balances, UncheckedState},
    campaign::{Active, Pricing, Validators},
    channel::Nonce,
    config::GANACHE_CONFIG,
    sentry::{
        message::Message, LastApproved, LastApprovedResponse, MessageResponse, SuccessResponse,
        ValidatorMessage, ValidatorMessagesListResponse, CLICK, IMPRESSION,
    },
    targeting::Rules,
    validator::{ApproveState, Heartbeat, MessageTypes, NewState, RejectState},
    AdUnit, Address, Campaign, Channel, EventSubmission, ToETHChecksum, UnifiedNum, ValidatorDesc,
    ValidatorId, IPFS,
};

use wiremock::{
    matchers::{method, path, query_param},
    Mock, MockGuard, MockServer, ResponseTemplate,
};

pub use logger::discard_logger;

/// [`ValidatorId`]s used for testing.
///
/// They are the same as the ones in Ganache and we know their keystore passphrases.
pub static IDS: Lazy<HashMap<Address, ValidatorId>> = Lazy::new(|| {
    vec![
        (*LEADER, LEADER.deref().into()),
        (*FOLLOWER, FOLLOWER.deref().into()),
        (*GUARDIAN, GUARDIAN.deref().into()),
        (*CREATOR, CREATOR.deref().into()),
        (*ADVERTISER, ADVERTISER.deref().into()),
        (*PUBLISHER, PUBLISHER.deref().into()),
        (*GUARDIAN_2, GUARDIAN_2.deref().into()),
        (*PUBLISHER_2, PUBLISHER_2.deref().into()),
        (*ADVERTISER_2, ADVERTISER_2.deref().into()),
        (*LEADER_2, LEADER_2.deref().into()),
    ]
    .into_iter()
    .collect()
});

pub static LEADER: Lazy<Address> = Lazy::new(|| *ADDRESS_0);
pub static FOLLOWER: Lazy<Address> = Lazy::new(|| *ADDRESS_1);
pub static GUARDIAN: Lazy<Address> = Lazy::new(|| *ADDRESS_2);
pub static CREATOR: Lazy<Address> = Lazy::new(|| *ADDRESS_3);
pub static ADVERTISER: Lazy<Address> = Lazy::new(|| *ADDRESS_4);
pub static PUBLISHER: Lazy<Address> = Lazy::new(|| *ADDRESS_5);
pub static GUARDIAN_2: Lazy<Address> = Lazy::new(|| *ADDRESS_6);
pub static PUBLISHER_2: Lazy<Address> = Lazy::new(|| *ADDRESS_7);
pub static ADVERTISER_2: Lazy<Address> = Lazy::new(|| *ADDRESS_8);
pub static LEADER_2: Lazy<Address> = Lazy::new(|| *ADDRESS_9);

/// passphrase: ganache0
pub static ADDRESS_0: Lazy<Address> = Lazy::new(|| {
    b"0x80690751969B234697e9059e04ed72195c3507fa"
        .try_into()
        .unwrap()
});

/// passphrase: ganache1
pub static ADDRESS_1: Lazy<Address> = Lazy::new(|| {
    b"0xf3f583AEC5f7C030722Fe992A5688557e1B86ef7"
        .try_into()
        .unwrap()
});

/// passphrase: ganache2
pub static ADDRESS_2: Lazy<Address> = Lazy::new(|| {
    b"0xe061E1EB461EaBE512759aa18A201B20Fe90631D"
        .try_into()
        .unwrap()
});

/// passphrase: ganache3
pub static ADDRESS_3: Lazy<Address> = Lazy::new(|| {
    b"0xaCBaDA2d5830d1875ae3D2de207A1363B316Df2F"
        .try_into()
        .unwrap()
});

/// passphrase: ganache4
pub static ADDRESS_4: Lazy<Address> = Lazy::new(|| {
    b"0xDd589B43793934EF6Ad266067A0d1D4896b0dff0"
        .try_into()
        .unwrap()
});

/// passphrase: ganache5
pub static ADDRESS_5: Lazy<Address> = Lazy::new(|| {
    b"0xE882ebF439207a70dDcCb39E13CA8506c9F45fD9"
        .try_into()
        .unwrap()
});

/// passphrase: ganache6
pub static ADDRESS_6: Lazy<Address> = Lazy::new(|| {
    b"0x79D358a3194d737880B3eFD94ADccD246af9F535"
        .try_into()
        .unwrap()
});

/// passphrase: ganache7
pub static ADDRESS_7: Lazy<Address> = Lazy::new(|| {
    b"0x0e880972A4b216906F05D67EeaaF55d16B5EE4F1"
        .try_into()
        .unwrap()
});

/// passphrase: ganache8
pub static ADDRESS_8: Lazy<Address> = Lazy::new(|| {
    b"0x541b401362Ea1D489D322579552B099e801F3632"
        .try_into()
        .unwrap()
});

/// passphrase: ganache9
pub static ADDRESS_9: Lazy<Address> = Lazy::new(|| {
    b"0x6B83e7D6B72c098d48968441e0d05658dc17Adb9"
        .try_into()
        .unwrap()
});

// Dummy adapter auth tokens
// authorization tokens
pub static DUMMY_AUTH: Lazy<HashMap<Address, String>> = Lazy::new(|| {
    let mut auth = HashMap::new();

    auth.insert(*LEADER, "AUTH_awesomeLeader".into());
    auth.insert(*FOLLOWER, "AUTH_awesomeFollower".into());
    auth.insert(*GUARDIAN, "AUTH_awesomeGuardian".into());
    auth.insert(*CREATOR, "AUTH_awesomeCreator".into());
    auth.insert(*ADVERTISER, "AUTH_awesomeAdvertiser".into());
    auth.insert(*PUBLISHER, "AUTH_awesomePublisher".into());
    auth.insert(*GUARDIAN_2, "AUTH_awesomeGuardian2".into());

    auth
});

mod logger {

    use slog::{o, Discard, Drain, Logger};
    pub fn discard_logger() -> Logger {
        let drain = Discard.fuse();

        Logger::root(drain, o!())
    }
}

pub static DUMMY_VALIDATOR_LEADER: Lazy<ValidatorDesc> = Lazy::new(|| ValidatorDesc {
    id: IDS[&LEADER],
    url: "http://localhost:8005".to_string(),
    fee: 100.into(),
    fee_addr: None,
});

pub static DUMMY_VALIDATOR_FOLLOWER: Lazy<ValidatorDesc> = Lazy::new(|| ValidatorDesc {
    id: IDS[&FOLLOWER],
    url: "http://localhost:8006".to_string(),
    fee: 100.into(),
    fee_addr: None,
});

/// Dummy Campaign uses Ganache #1337 with the mocked token
pub static DUMMY_CAMPAIGN: Lazy<Campaign> = Lazy::new(|| {
    let token_info = GANACHE_CONFIG
        .chains
        .get("Ganache #1337")
        .unwrap()
        .tokens
        .get("Mocked TOKEN")
        .unwrap();

    Campaign {
        id: "0x936da01f9abd4d9d80c702af85c822a8"
            .parse()
            .expect("Should parse"),
        channel: Channel {
            leader: IDS[&LEADER],
            follower: IDS[&FOLLOWER],
            guardian: *GUARDIAN,
            token: token_info.address,
            nonce: Nonce::from(987_654_321_u32),
        },
        creator: *CREATOR,
        // 1000.00000000
        budget: UnifiedNum::from(100_000_000_000),
        validators: Validators::new((
            DUMMY_VALIDATOR_LEADER.clone(),
            DUMMY_VALIDATOR_FOLLOWER.clone(),
        )),
        title: Some("Dummy Campaign".to_string()),
        pricing_bounds: vec![
            (
                IMPRESSION,
                Pricing {
                    min: 1.into(),
                    max: 10.into(),
                },
            ),
            (
                CLICK,
                Pricing {
                    min: 0.into(),
                    max: 0.into(),
                },
            ),
        ]
        .into_iter()
        .collect(),
        event_submission: Some(EventSubmission { allow: vec![] }),
        ad_units: vec![],
        targeting_rules: Rules::new(),
        created: Utc.ymd(2021, 2, 1).and_hms(7, 0, 0),
        active: Active {
            to: Utc.ymd(2099, 1, 30).and_hms(0, 0, 0),
            from: None,
        },
    }
});

pub static DUMMY_AD_UNITS: Lazy<[AdUnit; 4]> = Lazy::new(|| {
    [
        AdUnit {
            ipfs: IPFS::try_from("Qmasg8FrbuSQpjFu3kRnZF9beg8rEBFrqgi1uXDRwCbX5f")
                .expect("should convert"),
            media_url: "ipfs://QmcUVX7fvoLMM93uN2bD3wGTH8MXSxeL8hojYfL2Lhp7mR".to_string(),
            media_mime: "image/jpeg".to_string(),
            target_url: "https://www.adex.network/?stremio-test-banner-1".to_string(),
            archived: false,
            description: Some("Dummy AdUnit description 1".to_string()),
            ad_type: "legacy_250x250".to_string(),
            /// Timestamp: 1 564 383 600
            created: Utc.ymd(2019, 7, 29).and_hms(9, 0, 0),
            min_targeting_score: None,
            modified: None,
            owner: IDS[&PUBLISHER],
            title: Some("Dummy AdUnit 3".to_string()),
        },
        AdUnit {
            ipfs: IPFS::try_from("QmVhRDGXoM3Fg3HZD5xwMuxtb9ZErwC8wHt8CjsfxaiUbZ")
                .expect("should convert"),
            media_url: "ipfs://QmQB7uz7Gxfy7wqAnrnBcZFaVJLos8J9gn8mRcHQU6dAi1".to_string(),
            media_mime: "image/jpeg".to_string(),
            target_url: "https://www.adex.network/?adex-campaign=true&pub=stremio".to_string(),
            archived: false,
            description: Some("Dummy AdUnit description 2".to_string()),
            ad_type: "legacy_250x250".to_string(),
            /// Timestamp: 1 564 383 600
            created: Utc.ymd(2019, 7, 29).and_hms(9, 0, 0),
            min_targeting_score: None,
            modified: None,
            owner: IDS[&PUBLISHER],
            title: Some("Dummy AdUnit 2".to_string()),
        },
        AdUnit {
            ipfs: IPFS::try_from("QmYwcpMjmqJfo9ot1jGe9rfXsszFV1WbEA59QS7dEVHfJi")
                .expect("should convert"),
            media_url: "ipfs://QmQB7uz7Gxfy7wqAnrnBcZFaVJLos8J9gn8mRcHQU6dAi1".to_string(),
            media_mime: "image/jpeg".to_string(),
            target_url: "https://www.adex.network/?adex-campaign=true".to_string(),
            archived: false,
            description: Some("Dummy AdUnit description 3".to_string()),
            ad_type: "legacy_250x250".to_string(),
            /// Timestamp: 1 564 383 600
            created: Utc.ymd(2019, 7, 29).and_hms(9, 0, 0),
            min_targeting_score: None,
            modified: None,
            owner: IDS[&PUBLISHER],
            title: Some("Dummy AdUnit 3".to_string()),
        },
        AdUnit {
            ipfs: IPFS::try_from("QmTAF3FsFDS7Ru8WChoD9ofiHTH8gAQfR4mYSnwxqTDpJH")
                .expect("should convert"),
            media_url: "ipfs://QmQAcfBJpDDuH99A4p3pFtUmQwamS8UYStP5HxHC7bgYXY".to_string(),
            media_mime: "image/jpeg".to_string(),
            target_url: "https://adex.network".to_string(),
            archived: false,
            description: Some("Dummy AdUnit description 4".to_string()),
            ad_type: "legacy_250x250".to_string(),
            /// Timestamp: 1 564 383 600
            created: Utc.ymd(2019, 7, 29).and_hms(9, 0, 0),
            min_targeting_score: None,
            modified: None,
            owner: IDS[&PUBLISHER],
            title: Some("Dummy AdUnit 4".to_string()),
        },
    ]
});

// CID V0
pub static DUMMY_IPFS: Lazy<[IPFS; 5]> = Lazy::new(|| {
    [
        IPFS::try_from("QmcUVX7fvoLMM93uN2bD3wGTH8MXSxeL8hojYfL2Lhp7mR").expect("Valid IPFS V0"),
        IPFS::try_from("Qmasg8FrbuSQpjFu3kRnZF9beg8rEBFrqgi1uXDRwCbX5f").expect("Valid IPFS V0"),
        IPFS::try_from("QmQnu8zrHsuVvnTJsEgDHYA8c1MmRL7YLiMD8uzDUJKcNq").expect("Valid IPFS V0"),
        IPFS::try_from("QmYYBULc9QDEaDr8HAXvVWHDmFfL2GvyumYRr1g4ERBC96").expect("Valid IPFS V0"),
        // V1 of the V0 ipfs: `QmcUVX7fvoLMM93uN2bD3wGTH8MXSxeL8hojYfL2Lhp7mR`
        IPFS::try_from("bafybeif2h3mynaf3ylgdbs6arf6mczqycargt5cqm3rmel3wpjarlswway")
            .expect("Valid IPFS V1"),
    ]
});

pub struct ServerSetup {
    pub server: MockServer,
}

impl ServerSetup {
    pub async fn init(channel: &Channel) -> Self {
        let server = MockServer::start().await;
        let ok_response = SuccessResponse { success: true };

        // Making sure all propagations to leader/follower succeed
        Mock::given(method("POST"))
            .and(path(format!(
                "leader/v5/channel/{}/validator-messages",
                channel.id()
            )))
            .respond_with(ResponseTemplate::new(200).set_body_json(&ok_response))
            .mount(&server)
            .await;

        Mock::given(method("POST"))
            .and(path(format!(
                "follower/v5/channel/{}/validator-messages",
                channel.id()
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
                "/v5/channel/{}/validator-messages/{}/{}",
                DUMMY_CAMPAIGN.channel.id(),
                DUMMY_CAMPAIGN.channel.leader,
                "Heartbeat",
            )))
            .and(query_param("limit", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&heartbeat_res))
            .mount(&server)
            .await;

        Self { server }
    }

    pub async fn setup_new_state_response(
        &self,
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
                "/v5/channel/{}/validator-messages/{}/{}",
                DUMMY_CAMPAIGN.channel.id(),
                DUMMY_CAMPAIGN.channel.leader,
                "NewState",
            )))
            .and(query_param("limit", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&new_state_res))
            .expect(1)
            .named("GET NewState helper")
            .mount_as_scoped(&self.server)
            .await
    }

    // Gets wiremock to return a specific ApproveState message or None when called
    pub async fn setup_approve_state_response(
        &self,
        approve_state: Option<ApproveState>,
    ) -> MockGuard {
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
                DUMMY_CAMPAIGN.channel.leader,
                "ApproveState+RejectState",
            )))
            .and(query_param("limit", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&approve_state_res))
            .expect(1)
            .named("GET ApproveState helper")
            .mount_as_scoped(&self.server)
            .await
    }

    // Gets wiremock to return a specific RejectState message or None when called
    pub async fn setup_reject_state_response(
        &self,
        reject_state: Option<RejectState<UncheckedState>>,
    ) -> MockGuard {
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
                DUMMY_CAMPAIGN.channel.leader,
                "ApproveState+RejectState",
            )))
            .and(query_param("limit", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(&reject_state_res))
            .expect(1)
            .named("GET RejectState helper")
            .mount_as_scoped(&self.server)
            .await
    }

    pub async fn setup_last_approved_response(
        &self,
        balances: Balances<UncheckedState>,
        state_root: String,
    ) -> MockGuard {
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
            .expect(1)
            .named("GET LastApproved helper")
            .mount_as_scoped(&self.server)
            .await
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_ids() {
        println!("{:?}", IDS[&LEADER]);
    }
}
