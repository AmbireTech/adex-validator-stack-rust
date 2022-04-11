use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr},
};

use adapter::ethereum::{
    get_counterfactual_address,
    test_util::{
        GANACHE_INFO_1, GANACHE_INFO_1337, Outpace, Erc20Token, Sweeper,
    },
    Options,
};
use deposits::Deposit;
use once_cell::sync::Lazy;
use primitives::{
    config::GANACHE_CONFIG,
    test_util::{FOLLOWER, LEADER},
    util::ApiUrl,
    Address, Chain, Config,
};
use slog::{debug, Logger};
use web3::{transports::Http, Web3};

pub mod deposits;

/// ganache-cli setup with deployed contracts using the snapshot directory
/// NOTE: Current the snapshot and test setup use a single Chain.
///
/// Uses Chain #1337 from the [`GANACHE_CONFIG`] static to init the contracts
pub static SNAPSHOT_CONTRACTS_1337: Lazy<Contracts> = Lazy::new(|| {
    let ganache_chain_info = GANACHE_INFO_1337.clone();

    let web3 = Web3::new(
        Http::new(ganache_chain_info.chain.rpc.as_str()).expect("failed to init transport"),
    );

    let token_info = ganache_chain_info
        .tokens
        .get("Mocked TOKEN 1337")
        .expect("Ganache config should contain for Chain #1337 the Mocked TOKEN");
    let chain = ganache_chain_info.chain.clone();

    let token = Erc20Token::new(&web3, token_info.clone());

    let sweeper_address = Address::from(ganache_chain_info.chain.sweeper);

    let sweeper = Sweeper::new(&web3, sweeper_address);

    let outpace_address = Address::from(ganache_chain_info.chain.outpace);

    let outpace = Outpace::new(&web3, outpace_address);

    Contracts {
        token,
        sweeper,
        outpace,
        chain,
    }
});

/// Uses Chain #1 from the [`GANACHE_CONFIG`] static to init the contracts
pub static SNAPSHOT_CONTRACTS_1: Lazy<Contracts> = Lazy::new(|| {
    let ganache_chain_info = GANACHE_INFO_1.clone();

    let web3 = Web3::new(
        Http::new(ganache_chain_info.chain.rpc.as_str()).expect("failed to init transport"),
    );

    let token_info = ganache_chain_info
        .tokens
        .get("Mocked TOKEN 1")
        .expect("Ganache config should contain for Chain #1 the Mocked TOKEN");

    let token = Erc20Token::new(&web3, token_info.clone());

    let sweeper_address = Address::from(ganache_chain_info.chain.sweeper);

    let sweeper = Sweeper::new(&web3, sweeper_address);

    let outpace_address = Address::from(ganache_chain_info.chain.outpace);

    let outpace = Outpace::new(&web3, outpace_address);

    let chain = ganache_chain_info.chain.clone();

    Contracts {
        token,
        sweeper,
        outpace,
        chain,
    }
});

#[derive(Debug, Clone)]
pub struct TestValidator {
    pub address: Address,
    pub keystore: Options,
    pub sentry_config: sentry::application::Config,
    /// Sentry REST API url
    pub sentry_url: ApiUrl,
    /// Used for the _Sentry REST API_ [`sentry::Application`] as well as the _Validator worker_ [`validator_worker::Worker`]
    pub config: Config,
    /// Prefix for sentry logger
    pub sentry_logger_prefix: String,
    /// Prefix for validator worker logger
    pub worker_logger_prefix: String,
    /// Postgres DB name
    /// The rest of the Postgres values are taken from env. variables
    pub db_name: String,
}

pub static VALIDATORS: Lazy<HashMap<Address, TestValidator>> = Lazy::new(|| {
    use adapter::ethereum::test_util::KEYSTORES;
    use primitives::config::Environment;

    vec![
        (
            *LEADER,
            TestValidator {
                address: *LEADER,
                keystore: KEYSTORES[&LEADER].clone(),
                sentry_config: sentry::application::Config {
                    env: Environment::Development,
                    port: 8005,
                    ip_addr: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                    redis_url: "redis://127.0.0.1:6379/1".parse().unwrap(),
                },
                config: GANACHE_CONFIG.clone(),
                sentry_url: "http://localhost:8005".parse().expect("Valid Sentry URL"),
                sentry_logger_prefix: "sentry-leader".into(),
                worker_logger_prefix: "worker-leader".into(),
                db_name: "harness_leader".into(),
            },
        ),
        (
            *FOLLOWER,
            TestValidator {
                address: *FOLLOWER,
                keystore: KEYSTORES[&FOLLOWER].clone(),
                sentry_config: sentry::application::Config {
                    env: Environment::Development,
                    port: 8006,
                    ip_addr: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                    redis_url: "redis://127.0.0.1:6379/2".parse().unwrap(),
                },
                config: GANACHE_CONFIG.clone(),
                sentry_url: "http://localhost:8006".parse().expect("Valid Sentry URL"),
                sentry_logger_prefix: "sentry-follower".into(),
                worker_logger_prefix: "worker-follower".into(),
                db_name: "harness_follower".into(),
            },
        ),
    ]
    .into_iter()
    .collect()
});

pub struct Setup {
    pub chain: Chain,
    pub logger: Logger,
}

#[derive(Debug, Clone)]
pub struct Contracts {
    pub token: Erc20Token,
    #[deprecated = "We are removing the sweeper contract & the create2 addresses for Deposits"]
    pub sweeper: Sweeper,
    pub outpace: Outpace,
    pub chain: Chain,
}

impl Setup {
    pub async fn deploy_contracts(&self) -> Contracts {
        let transport = Http::new(self.chain.rpc.as_str()).expect("Invalid RPC for chain!");

        debug!(self.logger, "Preparing deployment of contracts to Chain {:?} on {}", self.chain.chain_id, self.chain.rpc; "chain" => ?self.chain);

        let web3 = Web3::new(transport);

        // deploy contracts
        // TOKEN contract is with precision 18 (like DAI)
        // set the minimum token units to 1 TOKEN
        let token = Erc20Token::deploy(&web3, 10_u64.pow(18))
            .await
            .expect("Correct parameters are passed to the Token constructor.");

        debug!(self.logger, "Deployed token contract"; "address" => ?token.info.address, "token_info" => ?token.info);

        let sweeper = Sweeper::deploy(&web3)
            .await
            .expect("Correct parameters are passed to the Sweeper constructor.");

        debug!(self.logger, "Deployed sweeper contract"; "address" => ?sweeper.address);

        let outpace = Outpace::deploy(&web3)
            .await
            .expect("Correct parameters are passed to the OUTPACE constructor.");

        debug!(self.logger, "Deployed outpace contract"; "address" => ?outpace.address);

        Contracts {
            token,
            sweeper,
            outpace,
            chain: self.chain.clone(),
        }
    }

    pub async fn deposit(&self, contracts: &Contracts, deposit: &Deposit) {
        let counterfactual_address = get_counterfactual_address(
            contracts.sweeper.address,
            &deposit.channel,
            contracts.outpace.address,
            deposit.address,
        );

        // OUTPACE regular deposit
        // first set a balance of tokens to be deposited
        contracts.token.set_balance(
            deposit.address.to_bytes(),
            deposit.address.to_bytes(),
            &deposit.outpace_amount,
        )
        .await
        .expect("Failed to set balance");
        // call the OUTPACE deposit
        contracts.outpace.deposit(
            &deposit.channel,
            deposit.address.to_bytes(),
            &deposit.outpace_amount,
        )
        .await
        .expect("Should deposit with OUTPACE");

        // Counterfactual address deposit
        contracts.token.set_balance(
            deposit.address.to_bytes(),
            counterfactual_address.to_bytes(),
            &deposit.counterfactual_amount,
        )
        .await
        .expect("Failed to set balance");
    }
}

#[cfg(test)]
mod tests {
    use crate::run::run_sentry_app;

    use super::*;
    use adapter::{
        ethereum::{
            test_util::{GANACHE_1, GANACHE_1337, KEYSTORES},
            UnlockedWallet,
        },
        prelude::*,
        primitives::ChainOf,
        Adapter, Ethereum,
    };
    use chrono::Utc;
    use primitives::{
        balances::{CheckedState, UncheckedState},
        sentry::{campaign_create::CreateCampaign, AccountingResponse, Event, SuccessResponse},
        spender::Spender,
        test_util::{ADVERTISER, DUMMY_AD_UNITS, DUMMY_IPFS, GUARDIAN, GUARDIAN_2, IDS, PUBLISHER},
        util::{logging::new_logger, ApiUrl},
        validator::{ApproveState, NewState},
        Balances, BigNum, Campaign, CampaignId, Channel, ChannelId, UnifiedNum,
    };
    use reqwest::{Client, StatusCode};
    use validator_worker::{worker::Worker, GetStateRoot, SentryApi};

    #[tokio::test]
    #[ignore = "We use a snapshot, however, we have left this test for convenience"]
    async fn deploy_contracts() {
        let logger = new_logger("test_harness");

        // Chain Id: 1
        {
            let setup = Setup {
                chain: GANACHE_1.clone(),
                logger: logger.clone(),
            };
            // deploy contracts
            let _contracts = setup.deploy_contracts().await;
        }

        // Chain Id: 1337
        {
            let setup = Setup {
                chain: GANACHE_1337.clone(),
                logger,
            };
            // deploy contracts
            let _contracts = setup.deploy_contracts().await;
        }
    }

    static CAMPAIGN_1: Lazy<Campaign> = Lazy::new(|| {
        use chrono::TimeZone;
        use primitives::{
            campaign::{Active, Pricing, PricingBounds, Validators},
            targeting::Rules,
            validator::ValidatorDesc,
            EventSubmission,
        };

        let channel = Channel {
            leader: VALIDATORS[&LEADER].address.into(),
            follower: VALIDATORS[&FOLLOWER].address.into(),
            guardian: *GUARDIAN,
            token: SNAPSHOT_CONTRACTS_1337.token.info.address,
            nonce: 0_u64.into(),
        };

        let leader_desc = ValidatorDesc {
            id: VALIDATORS[&LEADER].address.into(),
            url: VALIDATORS[&LEADER].sentry_url.to_string(),
            // min_validator_fee for token: 0.000_010
            // fee per 1000 (pro mille) = 0.00003000 (UnifiedNum)
            // fee per 1 payout: payout * fee / 1000 = payout * 0.00000003
            fee: 3_000.into(),
            fee_addr: None,
        };

        let follower_desc = ValidatorDesc {
            id: VALIDATORS[&FOLLOWER].address.into(),
            url: VALIDATORS[&FOLLOWER].sentry_url.to_string(),
            // min_validator_fee for token: 0.000_010
            // fee per 1000 (pro mille) = 0.00002000 (UnifiedNum)
            // fee per 1 payout: payout * fee / 1000 = payout * 0.00000002
            fee: 2_000.into(),
            fee_addr: None,
        };

        let validators = Validators::new((leader_desc, follower_desc));

        Campaign {
            id: "0x936da01f9abd4d9d80c702af85c822a8"
                .parse()
                .expect("Should parse"),
            channel,
            creator: *ADVERTISER,
            // 2.00000000
            budget: UnifiedNum::from(200_000_000),
            validators,
            title: Some("Dummy Campaign".to_string()),
            pricing_bounds: Some(PricingBounds {
                impression: Some(Pricing {
                    // 0.00003000
                    // Per 1000 = 0.03000000
                    min: 3_000.into(),
                    // 0.00005000
                    // Per 1000 = 0.05000000
                    max: 5_000.into(),
                }),
                click: Some(Pricing {
                    // 0.00006000
                    // Per 1000 = 0.06000000
                    min: 6_000.into(),
                    // 0.00010000
                    // Per 1000 = 0.10000000
                    max: 10_000.into(),
                }),
            }),
            event_submission: Some(EventSubmission { allow: vec![] }),
            ad_units: vec![DUMMY_AD_UNITS[0].clone(), DUMMY_AD_UNITS[1].clone()],
            targeting_rules: Rules::new(),
            created: Utc.ymd(2021, 2, 1).and_hms(7, 0, 0),
            active: Active {
                to: Utc.ymd(2099, 1, 30).and_hms(0, 0, 0),
                from: None,
            },
        }
    });

    /// This Campaign's Channel has switched leader & follower compared to [`CAMPAIGN_1`]
    ///
    /// `Channel.leader = VALIDATOR["follower"].address`
    /// `Channel.follower = VALIDATOR["leader"].address`
    /// See [`VALIDATORS`] for more details.
    static CAMPAIGN_2: Lazy<Campaign> = Lazy::new(|| {
        use chrono::TimeZone;
        use primitives::{
            campaign::{Active, Pricing, PricingBounds, Validators},
            targeting::Rules,
            validator::ValidatorDesc,
            EventSubmission,
        };

        let channel = Channel {
            leader: VALIDATORS[&FOLLOWER].address.into(),
            follower: VALIDATORS[&LEADER].address.into(),
            guardian: *GUARDIAN_2,
            token: SNAPSHOT_CONTRACTS_1337.token.info.address,
            nonce: 0_u64.into(),
        };

        // Uses the VALIDATORS[&FOLLOWER] as the Leader for this Channel
        // switches the URL as well
        let leader_desc = ValidatorDesc {
            id: VALIDATORS[&FOLLOWER].address.into(),
            url: VALIDATORS[&FOLLOWER].sentry_url.to_string(),
            // fee per 1000 (pro mille) = 0.10000000 (UnifiedNum)
            fee: 10_000_000.into(),
            fee_addr: None,
        };

        // Uses the VALIDATORS[&LEADER] as the Follower for this Channel
        // switches the URL as well
        let follower_desc = ValidatorDesc {
            id: VALIDATORS[&LEADER].address.into(),
            url: VALIDATORS[&LEADER].sentry_url.to_string(),
            // fee per 1000 (pro mille) = 0.05000000 (UnifiedNum)
            fee: 5_000_000.into(),
            fee_addr: None,
        };

        let validators = Validators::new((leader_desc, follower_desc));

        Campaign {
            id: "0x127b98248f4e4b73af409d10f62daeaa"
                .parse()
                .expect("Should parse"),
            channel,
            creator: *ADVERTISER,
            // 20.00000000
            budget: UnifiedNum::from(2_000_000_000),
            validators,
            title: Some("Dummy Campaign".to_string()),
            pricing_bounds: Some(PricingBounds {
                impression: Some(Pricing {
                    // 0.00001000
                    min: 1_000.into(),
                    // 0.00002000
                    max: 2_000.into(),
                }),
                click: Some(Pricing {
                    // 0.00003000
                    min: 3_000.into(),
                    // 0.00005000
                    max: 5_000.into(),
                }),
            }),
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

    /// This Campaign has a token from the GANACHE_1 chain instead of the GANACHE_1337 one like the others
    static CAMPAIGN_3: Lazy<Campaign> = Lazy::new(|| {
        use chrono::TimeZone;
        use primitives::{
            campaign::{Active, Pricing, PricingBounds, Validators},
            targeting::Rules,
            validator::ValidatorDesc,
            EventSubmission,
        };

        let channel = Channel {
            leader: VALIDATORS[&LEADER].address.into(),
            follower: VALIDATORS[&FOLLOWER].address.into(),
            guardian: *GUARDIAN_2,
            token: SNAPSHOT_CONTRACTS_1.token.info.address,
            nonce: 1_u64.into(),
        };

        let leader_desc = ValidatorDesc {
            id: VALIDATORS[&LEADER].address.into(),
            url: VALIDATORS[&LEADER].sentry_url.to_string(),
            // min_validator_fee for token: 0.000_010
            // fee per 1000 (pro mille) = 0.00003000 (UnifiedNum)
            // fee per 1 payout: payout * fee / 1000 = payout * 0.00000003
            fee: 3_000.into(),
            fee_addr: None,
        };

        let follower_desc = ValidatorDesc {
            id: VALIDATORS[&FOLLOWER].address.into(),
            url: VALIDATORS[&FOLLOWER].sentry_url.to_string(),
            // min_validator_fee for token: 0.000_010
            // fee per 1000 (pro mille) = 0.00002000 (UnifiedNum)
            // fee per 1 payout: payout * fee / 1000 = payout * 0.00000002
            fee: 2_000.into(),
            fee_addr: None,
        };

        let validators = Validators::new((leader_desc, follower_desc));

        Campaign {
            id: "0xa78f3492481b41a688488a7aa1ff17df"
                .parse()
                .expect("Should parse"),
            channel,
            creator: *ADVERTISER,
            // 20.00000000
            budget: UnifiedNum::from(2_000_000_000),
            validators,
            title: Some("Dummy Campaign in Chain #1".to_string()),
            pricing_bounds: Some(PricingBounds {
                impression: Some(Pricing {
                    // 0.00003000
                    // Per 1000 = 0.03000000
                    min: 3_000.into(),
                    // 0.00005000
                    // Per 1000 = 0.05000000
                    max: 5_000.into(),
                }),
                click: Some(Pricing {
                    // 0.00006000
                    // Per 1000 = 0.06000000
                    min: 6_000.into(),
                    // 0.00010000
                    // Per 1000 = 0.10000000
                    max: 10_000.into(),
                }),
            }),
            event_submission: Some(EventSubmission { allow: vec![] }),
            ad_units: vec![DUMMY_AD_UNITS[0].clone(), DUMMY_AD_UNITS[1].clone()],
            targeting_rules: Rules::new(),
            created: Utc.ymd(2021, 2, 1).and_hms(7, 0, 0),
            active: Active {
                to: Utc.ymd(2099, 1, 30).and_hms(0, 0, 0),
                from: None,
            },
        }
    });

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    // #[ignore = "for now"]
    async fn run_full_test() {
        let chain = GANACHE_1337.clone();
        assert_eq!(CAMPAIGN_1.channel.token, CAMPAIGN_2.channel.token);

        let token_chain_1337 = GANACHE_CONFIG
            .find_chain_of(CAMPAIGN_1.channel.token)
            .expect("Should find CAMPAIGN_1 channel token address in Config!");

        let second_token_chain = GANACHE_CONFIG
            .find_chain_of(CAMPAIGN_3.channel.token)
            .expect("Should find CAMPAIGN_3 channel token address in Config!");

        assert_eq!(&token_chain_1337.chain, &chain, "CAMPAIGN_1 & CAMPAIGN_2 should be both using the same #1337 Chain which is setup in the Ganache Config");

        let setup = Setup {
            chain: chain.clone(),
            logger: new_logger("test_harness"),
        };

        // Use snapshot contracts
        let contracts_1337 = SNAPSHOT_CONTRACTS_1337.clone();
        let contracts_1 = SNAPSHOT_CONTRACTS_1.clone();

        // let contracts = setup.deploy_contracts().await;

        let leader = VALIDATORS[&LEADER].clone();
        let follower = VALIDATORS[&FOLLOWER].clone();

        let token_1_precision = contracts_1.token.info.precision.get();
        let token_1337_precision = contracts_1337.token.info.precision.get();

        // We use the Advertiser's `EthereumAdapter::get_auth` for authentication!
        let advertiser_adapter = Adapter::new(
            Ethereum::init(KEYSTORES[&ADVERTISER].clone(), &GANACHE_CONFIG)
                .expect("Should initialize creator adapter"),
        )
        .unlock()
        .expect("Should unlock advertiser's Ethereum Adapter");

        // setup Sentry & returns Adapter
        let leader_adapter = setup_sentry(&leader)
            .await
            .unlock()
            .expect("Failed to unlock Leader ethereum adapter");
        let follower_adapter = setup_sentry(&follower)
            .await
            .unlock()
            .expect("Failed to unlock Follower ethereum adapter");

        let leader_sentry = SentryApi::new(
            leader_adapter.clone(),
            new_logger(&leader.worker_logger_prefix),
            leader.config.clone(),
            leader.sentry_url.clone(),
        )
        .expect("Should create new SentryApi for the Leader Worker");

        let follower_sentry = SentryApi::new(
            follower_adapter.clone(),
            new_logger(&follower.worker_logger_prefix),
            follower.config.clone(),
            follower.sentry_url.clone(),
        )
        .expect("Should create new SentryApi for the Leader Worker");

        let events = vec![
            Event::Impression {
                publisher: *PUBLISHER,
                ad_unit: Some(
                    CAMPAIGN_1
                        .ad_units
                        .get(0)
                        .expect("Should exist in Campaign")
                        .ipfs,
                ),
                ad_slot: Some(DUMMY_IPFS[2]),
                referrer: Some("https://adex.network".into()),
            },
            Event::Click {
                publisher: *PUBLISHER,
                ad_unit: Some(
                    CAMPAIGN_1
                        .ad_units
                        .get(0)
                        .expect("Should exist in Campaign")
                        .ipfs,
                ),
                ad_slot: Some(DUMMY_IPFS[2]),
                referrer: Some("https://ambire.com".into()),
            },
        ];

        // check Campaign Leader & Follower urls
        // they should be the same as the test validators
        {
            let campaign_leader_url = CAMPAIGN_1
                .leader()
                .expect("Channel.leader should match a Campaign validator!")
                .try_api_url()
                .expect("Valid url");
            let campaign_follower_url = CAMPAIGN_1
                .follower()
                .expect("Channel.follower should match a Campaign validator!")
                .try_api_url()
                .expect("Valid url");

            assert_eq!(&leader.sentry_url, &campaign_leader_url);
            assert_eq!(&follower.sentry_url, &campaign_follower_url);
        }

        // Advertiser deposits
        //
        // Channel 1 in Chain #1337:
        // - Outpace: 20 TOKENs
        // - Counterfactual: 10 TOKENs
        //
        // Channel 2 in Chain #1337:
        // - Outpace: 30 TOKENs
        // - Counterfactual: 20 TOKENs
        //
        // Channel 3 in Chain #1:
        // - Outpace: 30 TOKENS
        // - Counterfactual: 20 TOKENs
        {
            let advertiser_deposits = [
                Deposit {
                    channel: CAMPAIGN_1.channel,
                    token: contracts_1337.token.info.clone(),
                    address: advertiser_adapter.whoami().to_address(),
                    outpace_amount: BigNum::with_precision(20, token_1337_precision),
                    counterfactual_amount: BigNum::with_precision(10, token_1337_precision),
                },
                Deposit {
                    channel: CAMPAIGN_2.channel,
                    token: contracts_1337.token.info.clone(),
                    address: advertiser_adapter.whoami().to_address(),
                    outpace_amount: BigNum::with_precision(30, token_1337_precision),
                    counterfactual_amount: BigNum::with_precision(20, token_1337_precision),
                },
                Deposit {
                    channel: CAMPAIGN_3.channel,
                    token: contracts_1.token.info.clone(),
                    address: advertiser_adapter.whoami().to_address(),
                    outpace_amount: BigNum::with_precision(100, token_1_precision),
                    counterfactual_amount: BigNum::with_precision(20, token_1_precision),
                },
            ];

            // // 1st deposit
            // // Chain #1337
            // {
            //     setup
            //         .deposit(&contracts_1337, &advertiser_deposits[0])
            //         .await;

            //     // make sure we have the expected deposit returned from EthereumAdapter
            //     let eth_deposit = leader_adapter
            //         .get_deposit(
            //             &token_chain_1337.clone().with_channel(CAMPAIGN_1.channel),
            //             advertiser_adapter.whoami().to_address(),
            //         )
            //         .await
            //         .expect("Should get deposit for advertiser");

            //     assert_eq!(advertiser_deposits[0], eth_deposit);
            // }

            // // 2nd deposit
            // // Chain #1337
            // {
            //     setup
            //         .deposit(&contracts_1337, &advertiser_deposits[1])
            //         .await;

            //     // make sure we have the expected deposit returned from EthereumAdapter
            //     let eth_deposit = leader_adapter
            //         .get_deposit(
            //             &token_chain_1337.clone().with_channel(CAMPAIGN_2.channel),
            //             advertiser_adapter.whoami().to_address(),
            //         )
            //         .await
            //         .expect("Should get deposit for advertiser");

            //     assert_eq!(advertiser_deposits[1], eth_deposit);
            // }

            // 3rd deposit
            // Chain #1
            {
                setup.deposit(&contracts_1, &advertiser_deposits[2]).await;

                // make sure we have the expected deposit returned from EthereumAdapter
                let eth_deposit = leader_adapter
                    .get_deposit(
                        &second_token_chain.clone().with_channel(CAMPAIGN_3.channel),
                        advertiser_adapter.whoami().to_address(),
                    )
                    .await
                    .expect("Should get deposit for advertiser");

                assert_eq!(advertiser_deposits[2], eth_deposit);
            }
        }

        let api_client = reqwest::Client::new();

        // No Channel 1 - 404
        // GET /v5/channel/{}/spender/all
        {
            let leader_auth = advertiser_adapter
                .get_auth(chain.chain_id, leader_adapter.whoami())
                .expect("Get authentication");

            let leader_response = get_spender_all_page_0(
                &api_client,
                &leader.sentry_url,
                &leader_auth,
                CAMPAIGN_1.channel.id(),
            )
            .await
            .expect("Should return Response");

            assert_eq!(StatusCode::NOT_FOUND, leader_response.status());
        }

        // Create Campaign 1 w/ Channel 1 using Advertiser
        // Response: 400 - not enough deposit
        // Channel 1 - Is created, even though campaign creation failed.
        // POST /v5/campaign
        {
            let leader_auth = advertiser_adapter
                .get_auth(chain.chain_id, leader_adapter.whoami())
                .expect("Get authentication");

            let mut no_budget_campaign = CreateCampaign::from_campaign(CAMPAIGN_1.clone());
            // Deposit of Advertiser for Channel 2: 20 (outpace) + 10 (create2)
            // Campaign Budget: 40 TOKENs
            no_budget_campaign.budget = UnifiedNum::from(4_000_000_000);

            let no_budget_response = create_campaign(
                &api_client,
                &leader.sentry_url,
                &leader_auth,
                &no_budget_campaign,
            )
            .await
            .expect("Should return Response");
            let status = no_budget_response.status();
            let response = no_budget_response
                .json::<serde_json::Value>()
                .await
                .expect("Deserialization");

            assert_eq!(StatusCode::BAD_REQUEST, status);
            let expected_error = serde_json::json!({
                "message": "Not enough deposit left for the new campaign's budget"
            });

            assert_eq!(expected_error, response);
        }

        // Channel 1 - 200
        // Exists from the previously failed create Campaign 1 request
        // GET /v5/channel/{}/spender/all
        {
            let leader_response = leader_sentry
                .get_all_spenders(&token_chain_1337.clone().with_channel(CAMPAIGN_1.channel))
                .await
                .expect("Should return Response");

            let expected = vec![(
                advertiser_adapter.whoami().to_address(),
                Spender {
                    // Expected: 30 TOKENs
                    total_deposited: UnifiedNum::from(3_000_000_000),
                    total_spent: None,
                },
            )]
            .into_iter()
            .collect::<HashMap<_, _>>();

            assert_eq!(expected, leader_response);
        }

        // Create Campaign 1 w/ Channel 1 using Advertiser
        // In Leader & Follower sentries
        // Response: 200 Ok
        {
            let create_campaign_1 = CreateCampaign::from_campaign(CAMPAIGN_1.clone());
            {
                let leader_token = advertiser_adapter
                    .get_auth(chain.chain_id, leader_adapter.whoami())
                    .expect("Get authentication");

                let leader_response = create_campaign(
                    &api_client,
                    &leader.sentry_url,
                    &leader_token,
                    &create_campaign_1,
                )
                .await
                .expect("Should return Response");

                assert_eq!(StatusCode::OK, leader_response.status());
            }

            {
                let follower_token = advertiser_adapter
                    .get_auth(chain.chain_id, follower_adapter.whoami())
                    .expect("Get authentication");

                let follower_response = create_campaign(
                    &api_client,
                    &follower.sentry_url,
                    &follower_token,
                    &create_campaign_1,
                )
                .await
                .expect("Should return Response");

                assert_eq!(StatusCode::OK, follower_response.status());
            }
        }

        // Create Campaign 2 w/ Channel 2 using Advertiser
        // In Leader & Follower sentries
        // Response: 200 Ok
        // POST /v5/campaign
        {
            let create_campaign_2 = CreateCampaign::from_campaign(CAMPAIGN_2.clone());

            {
                let leader_token = advertiser_adapter
                    .get_auth(chain.chain_id, leader_adapter.whoami())
                    .expect("Get authentication");

                let leader_response = create_campaign(
                    &api_client,
                    &leader.sentry_url,
                    &leader_token,
                    &create_campaign_2,
                )
                .await
                .expect("Should return Response");
                let status = leader_response.status();

                assert_eq!(StatusCode::OK, status);
            }

            {
                let follower_token = advertiser_adapter
                    .get_auth(token_chain_1337.chain.chain_id, follower_adapter.whoami())
                    .expect("Get authentication");

                let follower_response = create_campaign(
                    &api_client,
                    &follower.sentry_url,
                    &follower_token,
                    &create_campaign_2,
                )
                .await
                .expect("Should return Response");

                assert_eq!(StatusCode::OK, follower_response.status());
            }
        }

        // Create Campaign 3 w/ Channel 3 using Advertiser on a different chain
        // In Leader & Follower sentries
        // Response: 200 Ok
        // POST /v5/campaign
        {
            let second_chain = GANACHE_1.clone();
            let create_campaign_3 = CreateCampaign::from_campaign(CAMPAIGN_3.clone());

            assert_eq!(
                &second_token_chain.chain, &second_chain,
                "CAMPAIGN_3 should be using the #1 Chain which is setup in the Ganache Config"
            );

            {
                let leader_token = advertiser_adapter
                    .get_auth(second_chain.chain_id, leader_adapter.whoami())
                    .expect("Get authentication");

                let leader_response = create_campaign(
                    &api_client,
                    &leader.sentry_url,
                    &leader_token,
                    &create_campaign_3,
                )
                .await
                .expect("Should return Response");

                let status = leader_response.status();
                assert_eq!(StatusCode::OK, status, "Creating CAMPAIGN_3 failed");
            }

            {
                let follower_token = advertiser_adapter
                    .get_auth(second_chain.chain_id, follower_adapter.whoami())
                    .expect("Get authentication");

                let follower_response = create_campaign(
                    &api_client,
                    &follower.sentry_url,
                    &follower_token,
                    &create_campaign_3,
                )
                .await
                .expect("Should return Response");

                assert_eq!(
                    StatusCode::OK,
                    follower_response.status(),
                    "Creating CAMPAIGN_3 failed"
                );
            }
        }

        let leader_worker = Worker::from_sentry(leader_sentry.clone());
        let follower_worker = Worker::from_sentry(follower_sentry.clone());

        // leader single worker tick
        leader_worker.all_channels_tick().await;
        // follower single worker tick
        follower_worker.all_channels_tick().await;

        // Channel 1 expected Accounting - Empty
        {
            let expected_accounting = AccountingResponse {
                balances: Balances::<CheckedState>::new(),
            };
            let actual_accounting = leader_sentry
                .get_accounting(&token_chain_1337.clone().with_channel(CAMPAIGN_1.channel))
                .await
                .expect("Should get Channel Accounting");

            assert_eq!(expected_accounting, actual_accounting);
        }

        // Add new events to sentry
        {
            let response = post_new_events(
                &leader_sentry,
                token_chain_1337.clone().with(CAMPAIGN_1.id),
                &events,
            )
            .await
            .expect("Posted events");

            assert_eq!(SuccessResponse { success: true }, response)
        }

        // Channel 1 expected Accounting
        // Fees are calculated based on pro mile of the payout
        // event payout * fee / 1000
        //
        //
        // IMPRESSION:
        // - Publisher payout: 3000
        // - Leader fees: 3000 * 3000 / 1 000 = 9 000
        // - Follower fees: 3000 * 2000 / 1000 = 6 000
        //
        // CLICK:
        // - Publisher payout: 6000
        // - Leader fees: 6000 * 3000 / 1000 = 18 000
        // - Follower fees: 6000 * 2000 / 1000 = 12 000
        //
        // Creator (Advertiser) pays out:
        // events_payout + leader fee + follower fee
        // events_payout = 3000 (impression) + 6000 (click) = 9 000
        // 9000 + (9000 + 18000) + (6000 + 12000) = 54 000
        {
            let mut expected_balances = Balances::new();

            expected_balances
                .spend(
                    CAMPAIGN_1.creator,
                    CAMPAIGN_1.channel.leader.to_address(),
                    UnifiedNum::from(27_000),
                )
                .expect("Should spend for Leader");
            expected_balances
                .spend(
                    CAMPAIGN_1.creator,
                    CAMPAIGN_1.channel.follower.to_address(),
                    UnifiedNum::from(18_000),
                )
                .expect("Should spend for Follower");
            expected_balances
                .spend(CAMPAIGN_1.creator, *PUBLISHER, UnifiedNum::from(9_000))
                .expect("Should spend for Publisher");

            let expected_accounting = AccountingResponse {
                balances: expected_balances,
            };

            let actual_accounting = leader_sentry
                .get_accounting(&token_chain_1337.with_channel(CAMPAIGN_1.channel))
                .await
                .expect("Should get Channel Accounting");

            pretty_assertions::assert_eq!(expected_accounting, actual_accounting);
        }

        // Running validator worker tests
        test_leader_and_follower_loop(
            leader_sentry,
            follower_sentry,
            leader_worker,
            follower_worker,
            events,
        )
        .await;
    }

    async fn setup_sentry(validator: &TestValidator) -> adapter::ethereum::LockedAdapter {
        let adapter = Adapter::new(
            Ethereum::init(validator.keystore.clone(), &GANACHE_CONFIG)
                .expect("EthereumAdapter::init"),
        );

        run_sentry_app(adapter.clone(), &validator)
            .await
            .expect("To run Sentry API server");

        adapter
    }

    /// Used to test if it returns correct Status code on non-existing Channel.
    async fn get_spender_all_page_0(
        api_client: &Client,
        url: &ApiUrl,
        token: &str,
        channel: ChannelId,
    ) -> anyhow::Result<reqwest::Response> {
        let endpoint_url = url
            .join(&format!("v5/channel/{}/spender/all", channel))
            .expect("valid endpoint");

        Ok(api_client
            .get(endpoint_url)
            .bearer_auth(&token)
            .send()
            .await?)
    }

    /// Used to test if it returns correct Status code on non-existing Channel.
    /// Authentication required!
    /// Asserts: [`StatusCode::OK`]
    async fn post_new_events<C: Unlocked + 'static>(
        sentry: &SentryApi<C, ()>,
        campaign_context: ChainOf<CampaignId>,
        events: &[Event],
    ) -> anyhow::Result<SuccessResponse> {
        let endpoint_url = sentry
            .sentry_url
            .join(&format!("v5/campaign/{}/events", campaign_context.context))
            .expect("valid endpoint");

        let request_body = vec![("events".to_string(), events)]
            .into_iter()
            .collect::<HashMap<_, _>>();

        let auth_token = sentry
            .adapter
            .get_auth(campaign_context.chain.chain_id, sentry.adapter.whoami())?;

        let response = sentry
            .client
            .post(endpoint_url)
            .json(&request_body)
            .bearer_auth(&auth_token)
            .send()
            .await?;

        assert_eq!(StatusCode::OK, response.status());

        Ok(response.json().await?)
    }

    /// Authentication required!
    async fn create_campaign(
        api_client: &Client,
        url: &ApiUrl,
        token: &str,
        create_campaign: &CreateCampaign,
    ) -> anyhow::Result<reqwest::Response> {
        let endpoint_url = url.join("v5/campaign").expect("valid endpoint");

        Ok(api_client
            .post(endpoint_url)
            .json(create_campaign)
            .bearer_auth(token)
            .send()
            .await?)
    }

    async fn test_leader_and_follower_loop<C: Unlocked + 'static>(
        leader_sentry: SentryApi<C, ()>,
        follower_sentry: SentryApi<C, ()>,
        leader_worker: Worker<Ethereum<UnlockedWallet>>,
        follower_worker: Worker<Ethereum<UnlockedWallet>>,
        events: Vec<Event>,
    ) {
        let token_chain_1337 = GANACHE_CONFIG
            .find_chain_of(CAMPAIGN_1.channel.token)
            .expect("Should find CAMPAIGN_1 channel token address in Config!");

        let mut new_channel = CAMPAIGN_1.channel.clone();
        new_channel.nonce = 1_u64.into();
        let mut campaign = CAMPAIGN_1.clone();
        campaign.channel = new_channel;
        // Use snapshot contracts
        let contracts_1337 = SNAPSHOT_CONTRACTS_1337.clone();

        let leader = VALIDATORS[&LEADER].clone();
        let follower = VALIDATORS[&FOLLOWER].clone();

        let create_campaign_1 = CreateCampaign::from_campaign(campaign.clone());
        let api_client = reqwest::Client::new();
        let advertiser_adapter = Adapter::new(
            Ethereum::init(KEYSTORES[&ADVERTISER].clone(), &GANACHE_CONFIG)
                .expect("Should initialize creator adapter"),
        )
        .unlock()
        .expect("Should unlock advertiser's Ethereum Adapter");
        let context_of_channel = token_chain_1337.clone().with(new_channel);
        let leader_token = advertiser_adapter
            .get_auth(context_of_channel.chain.chain_id, IDS[&LEADER])
            .expect("Get authentication");

        let follower_token = advertiser_adapter
            .get_auth(context_of_channel.chain.chain_id, IDS[&FOLLOWER])
            .expect("Get authentication");
        create_campaign(
            &api_client,
            &leader.sentry_url,
            &leader_token,
            &create_campaign_1,
        )
        .await
        .expect("Should return Response");

        create_campaign(
            &api_client,
            &follower.sentry_url,
            &follower_token,
            &create_campaign_1,
        )
        .await
        .expect("Should return Response");

        // leader::tick() accepts a sentry instance with validators to propagate to
        let leader_sentry_with_propagate = {
            let all_channels_validators = leader_sentry
                .collect_channels()
                .await
                .expect("Should collect channels");

            leader_sentry
                .clone()
                .with_propagate(all_channels_validators.1)
                .expect("Should get sentry")
        };

        // follower::tick() accepts a sentry instance with validators to propagate to
        let follower_sentry_with_propagate = {
            let all_channels_validators = follower_sentry
                .collect_channels()
                .await
                .expect("Should collect channels");

            follower_sentry
                .clone()
                .with_propagate(all_channels_validators.1)
                .expect("Should get sentry")
        };

        // Testing propagation and retrieval of NewState messages, verification of balances
        // We make a NewState message, propagate it, update the balances and send a second message with the new balances
        {
            let mut accounting_balances = get_test_accounting_balances();

            // Posting new events
            post_new_events(
                &leader_sentry,
                token_chain_1337.clone().with(CAMPAIGN_1.id),
                &events,
            )
            .await
            .expect("Posted events");
            post_new_events(
                &follower_sentry,
                token_chain_1337.clone().with(CAMPAIGN_1.id),
                &events,
            )
            .await
            .expect("Posted events");

            // leader single worker tick
            leader_worker.all_channels_tick().await;
            // follower single worker tick
            follower_worker.all_channels_tick().await;

            // Retrieving the NewState message from both validators
            let newstate = leader_sentry_with_propagate
                .get_our_latest_msg(new_channel.id(), &["NewState"])
                .await
                .expect("should fetch")
                .unwrap();
            let newstate_follower = follower_sentry_with_propagate
                .get_our_latest_msg(new_channel.id(), &["NewState"])
                .await
                .expect("should fetch")
                .unwrap();
            let heartbeat = leader_sentry_with_propagate
                .get_our_latest_msg(new_channel.id(), &["Heartbeat"])
                .await
                .expect("should fetch")
                .unwrap();
            println!("Heartbeat - {:?}", heartbeat);

            let newstate = NewState::<CheckedState>::try_from(newstate).expect("Should convert");
            let newstate_follower =
                NewState::<CheckedState>::try_from(newstate_follower).expect("Should convert");

            assert_eq!(
                newstate.state_root, newstate_follower.state_root,
                "Leader/Follower NewStates match"
            );
            let mut expected_balances = Balances::new();
            expected_balances
                .spend(
                    CAMPAIGN_1.creator,
                    CAMPAIGN_1.channel.leader.to_address(),
                    UnifiedNum::from_u64(27000),
                )
                .expect("Should spend");
            expected_balances
                .spend(
                    CAMPAIGN_1.creator,
                    CAMPAIGN_1.channel.follower.to_address(),
                    UnifiedNum::from_u64(18000),
                )
                .expect("Should spend");
            expected_balances
                .spend(CAMPAIGN_1.creator, *PUBLISHER, UnifiedNum::from_u64(9000))
                .expect("Should spend");

            assert_eq!(
                newstate.balances, expected_balances,
                "Balances are as expected"
            );

            // Balances are being changed since the last propagated message ensuring that a new NewState will be generated
            accounting_balances
                .spend(campaign.creator, *PUBLISHER, UnifiedNum::from(9_000))
                .expect("Should spend for Publisher");
            let new_state = get_new_state_msg(
                &leader_sentry,
                &accounting_balances,
                contracts_1337.token.info.precision.get(),
            );

            // Posting new events
            post_new_events(
                &leader_sentry,
                token_chain_1337.clone().with(CAMPAIGN_1.id),
                &events,
            )
            .await
            .expect("Posted events");
            post_new_events(
                &follower_sentry,
                token_chain_1337.clone().with(CAMPAIGN_1.id),
                &events,
            )
            .await
            .expect("Posted events");

            // leader single worker tick
            leader_worker.all_channels_tick().await;
            // follower single worker tick
            follower_worker.all_channels_tick().await;

            let newstate = leader_sentry_with_propagate
                .get_our_latest_msg(new_channel.id(), &["NewState"])
                .await
                .expect("should fetch")
                .unwrap();

            let newstate_follower = follower_sentry_with_propagate
                .get_our_latest_msg(new_channel.id(), &["NewState"])
                .await
                .expect("should fetch")
                .unwrap();

            let newstate = NewState::<CheckedState>::try_from(newstate).expect("Should convert");
            let newstate_follower =
                NewState::<CheckedState>::try_from(newstate_follower).expect("Should convert");

            assert_eq!(
                newstate.state_root, newstate_follower.state_root,
                "Stateroots of the new messages match"
            );
            let mut expected_balances = Balances::new();
            expected_balances
                .spend(
                    CAMPAIGN_1.creator,
                    CAMPAIGN_1.channel.leader.to_address(),
                    UnifiedNum::from_u64(27000),
                )
                .expect("Should spend");
            expected_balances
                .spend(
                    CAMPAIGN_1.creator,
                    CAMPAIGN_1.channel.follower.to_address(),
                    UnifiedNum::from_u64(18000),
                )
                .expect("Should spend");
            expected_balances
                .spend(CAMPAIGN_1.creator, *PUBLISHER, UnifiedNum::from_u64(18000))
                .expect("Should spend");
            assert_eq!(
                newstate.balances, expected_balances,
                "Balances are as expected"
            );
        }

        // Testing ApproveState propagation, ensures the validator worker and follower are running properly
        // We propagate a NewState/ApproveState pair, verify they match, then we update
        {
            let mut accounting_balances = get_test_accounting_balances();

            let new_state = get_new_state_msg(
                &leader_sentry,
                &accounting_balances,
                contracts_1337.token.info.precision.get(),
            );

            let approve_state = ApproveState {
                state_root: new_state.state_root.clone(),
                signature: new_state.signature.clone(),
                is_healthy: true,
            };

            // Posting new events
            post_new_events(
                &leader_sentry,
                token_chain_1337.clone().with(CAMPAIGN_1.id),
                &events,
            )
            .await
            .expect("Posted events");
            post_new_events(
                &follower_sentry,
                token_chain_1337.clone().with(CAMPAIGN_1.id),
                &events,
            )
            .await
            .expect("Posted events");

            // leader single worker tick
            leader_worker.all_channels_tick().await;
            // follower single worker tick
            follower_worker.all_channels_tick().await;

            let res = follower_sentry_with_propagate
                .get_last_approved(new_channel.id())
                .await
                .expect("should retrieve");
            assert!(res.last_approved.is_some(), "We have a last_approved pair");
            let last_approved = res.last_approved.unwrap();
            assert!(
                last_approved.new_state.is_some(),
                "We have a new_state in last_approved"
            );
            assert!(
                last_approved.approve_state.is_some(),
                "We have approve_state in last_approved"
            );
            let new_state_root = &last_approved.new_state.unwrap().msg.state_root;
            let approve_state_root = &last_approved.approve_state.unwrap().msg.state_root;
            assert_eq!(
                new_state_root, approve_state_root,
                "NewState and ApproveState state roots match"
            );

            accounting_balances
                .spend(campaign.creator, *PUBLISHER, UnifiedNum::from(9_000))
                .expect("Should spend for Publisher");

            // Propagating a new NewState so that the follower has to generate an ApproveState message
            let new_state = get_new_state_msg(
                &leader_sentry,
                &accounting_balances,
                contracts_1337.token.info.precision.get(),
            );

            let approve_state = ApproveState {
                state_root: new_state.state_root.clone(),
                signature: new_state.signature.clone(),
                is_healthy: true,
            };

            // Posting new events
            post_new_events(
                &leader_sentry,
                token_chain_1337.clone().with(CAMPAIGN_1.id),
                &events,
            )
            .await
            .expect("Posted events");
            post_new_events(
                &follower_sentry,
                token_chain_1337.clone().with(CAMPAIGN_1.id),
                &events,
            )
            .await
            .expect("Posted events");

            // leader single worker tick
            leader_worker.all_channels_tick().await;
            // follower single worker tick
            follower_worker.all_channels_tick().await;

            // leader single worker tick
            leader_worker.all_channels_tick().await;
            // follower single worker tick
            follower_worker.all_channels_tick().await;

            let res = follower_sentry_with_propagate
                .get_last_approved(new_channel.id())
                .await
                .expect("should retrieve");
            assert!(res.last_approved.is_some(), "We have a last_approved");
            let new_last_approved = res.last_approved.unwrap();

            assert_ne!(
                &new_last_approved.new_state.unwrap().msg.state_root,
                new_state_root,
                "NewState is different from the last pair"
            );
            assert_ne!(
                &new_last_approved.approve_state.unwrap().msg.state_root,
                approve_state_root,
                "ApproveState is different from the last pair"
            );
        }
    }

    fn get_new_state_msg<C: Unlocked + 'static>(
        sentry: &SentryApi<C, ()>,
        accounting_balances: &Balances,
        precision: u8,
    ) -> NewState<UncheckedState> {
        let state_root = accounting_balances
            .encode(CAMPAIGN_1.channel.id(), precision)
            .expect("should encode");
        let signature = sentry.adapter.sign(&state_root).expect("should sign");
        NewState {
            state_root: state_root.to_string(),
            signature,
            balances: accounting_balances.clone().into_unchecked(),
        }
    }

    fn get_test_accounting_balances() -> Balances {
        let mut accounting_balances = Balances::new();
        accounting_balances
            .spend(
                CAMPAIGN_1.creator,
                CAMPAIGN_1.channel.leader.to_address(),
                UnifiedNum::from(27_000),
            )
            .expect("Should spend for Leader");
        accounting_balances
            .spend(
                CAMPAIGN_1.creator,
                CAMPAIGN_1.channel.follower.to_address(),
                UnifiedNum::from(18_000),
            )
            .expect("Should spend for Follower");
        accounting_balances
            .spend(CAMPAIGN_1.creator, *PUBLISHER, UnifiedNum::from(9_000))
            .expect("Should spend for Publisher");
        accounting_balances
    }
}
pub mod run {
    use std::{env::current_dir, net::SocketAddr, path::PathBuf};

    use primitives::{
        postgres::{POSTGRES_HOST, POSTGRES_PASSWORD, POSTGRES_PORT, POSTGRES_USER},
        util::logging::new_logger,
        ToETHChecksum, ValidatorId,
    };
    use sentry::{
        db::{
            postgres_connection, redis_connection, redis_pool::Manager,
            tests_postgres::setup_test_migrations, CampaignRemaining,
        },
        Application,
    };
    use slog::info;
    use subprocess::{Popen, PopenConfig, Redirection};

    use crate::{TestValidator, GANACHE_CONFIG};

    pub async fn run_sentry_app(
        adapter: adapter::ethereum::LockedAdapter,
        validator: &TestValidator,
    ) -> anyhow::Result<()> {
        let socket_addr = SocketAddr::new(
            validator.sentry_config.ip_addr,
            validator.sentry_config.port,
        );

        let postgres_config = {
            let mut config = sentry::db::PostgresConfig::new();

            config
                .user(POSTGRES_USER.as_str())
                .password(POSTGRES_PASSWORD.as_str())
                .host(POSTGRES_HOST.as_str())
                .port(*POSTGRES_PORT)
                .dbname(&validator.db_name);

            config
        };

        let postgres = postgres_connection(42, postgres_config).await?;
        let mut redis = redis_connection(validator.sentry_config.redis_url.clone()).await?;

        Manager::flush_db(&mut redis)
            .await
            .expect("Should flush redis database");

        let campaign_remaining = CampaignRemaining::new(redis.clone());

        let app = Application::new(
            adapter,
            GANACHE_CONFIG.clone(),
            new_logger(&validator.sentry_logger_prefix),
            redis.clone(),
            postgres.clone(),
            campaign_remaining,
        );

        // Before the tests, make sure to flush the DB from previous run of `sentry` tests
        Manager::flush_db(&mut redis)
            .await
            .expect("Should flush redis database");

        setup_test_migrations(postgres.clone())
            .await
            .expect("Should run migrations");

        info!(&app.logger, "Spawn sentry Hyper server");
        tokio::spawn(app.run(socket_addr));

        Ok(())
    }
    /// This helper function generates the correct file path to a project file from the current one.
    ///
    /// The `file_path` starts from the Cargo workspace directory.
    fn project_file_path(file_path: &str) -> PathBuf {
        let full_path = current_dir().unwrap();
        let project_path = full_path.parent().unwrap().to_path_buf();

        project_path.join(file_path)
    }

    /// ```bash
    /// POSTGRES_DB=sentry_leader PORT=8005 KEYSTORE_PWD=address1 \
    /// cargo run -p sentry -- --adapter ethereum --keystoreFile ./adapter/test/resources/0x5a04A8fB90242fB7E1db7d1F51e268A03b7f93A5_keystore.json \
    /// ./docs/config/ganache.toml
    /// ```
    ///
    /// The identity is used to get the correct Keystore file
    /// While the password is passed to `sentry` with environmental variable
    pub fn run_sentry(keystore_password: &str, identity: ValidatorId) -> anyhow::Result<Popen> {
        let keystore_file_name = format!(
            "adapter/test/resources/{}_keystore.json",
            identity.to_checksum()
        );
        let keystore_path = project_file_path(&keystore_file_name);
        let ganache_config_path = project_file_path("docs/config/ganache.toml");

        let sentry_leader = Popen::create(
            &[
                "cargo",
                "run",
                "-p",
                "sentry",
                "--",
                "--adapter",
                "ethereum",
                "--keystoreFile",
                &keystore_path.to_string_lossy(),
                &ganache_config_path.to_string_lossy(),
            ],
            PopenConfig {
                stdout: Redirection::Pipe,
                env: Some(vec![
                    ("PORT".parse().unwrap(), "8005".parse().unwrap()),
                    (
                        "POSTGRES_DB".parse().unwrap(),
                        "sentry_leader".parse().unwrap(),
                    ),
                    (
                        "KEYSTORE_PWD".parse().unwrap(),
                        keystore_password.parse().unwrap(),
                    ),
                ]),
                ..Default::default()
            },
        )?;

        Ok(sentry_leader)
    }
}
