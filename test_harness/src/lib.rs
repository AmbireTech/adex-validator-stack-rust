#![allow(deprecated)]
use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr},
};

use adapter::ethereum::{
    get_counterfactual_address,
    test_util::{Erc20Token, Outpace, Sweeper, GANACHE_INFO_1, GANACHE_INFO_1337},
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
        .expect("Ganache config should contain for Chain #1337 the Mocked TOKEN 1337");
    let chain = ganache_chain_info.chain.clone();

    let token = Erc20Token::new(&web3, token_info.clone());

    let sweeper_address = ganache_chain_info.chain.sweeper;

    let sweeper = Sweeper::new(&web3, sweeper_address);

    let outpace_address = ganache_chain_info.chain.outpace;

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
        .expect("Ganache config should contain for Chain #1 the Mocked TOKEN 1");

    let token = Erc20Token::new(&web3, token_info.clone());

    let sweeper_address = ganache_chain_info.chain.sweeper;

    let sweeper = Sweeper::new(&web3, sweeper_address);

    let outpace_address = ganache_chain_info.chain.outpace;

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
    pub sentry_config: sentry::application::EnvConfig,
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
                sentry_config: sentry::application::EnvConfig {
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
                sentry_config: sentry::application::EnvConfig {
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
        contracts
            .token
            .set_balance(
                deposit.address.to_bytes(),
                deposit.address.to_bytes(),
                &deposit.outpace_amount,
            )
            .await
            .expect("Failed to set balance");
        // call the OUTPACE deposit
        contracts
            .outpace
            .deposit(
                &deposit.channel,
                deposit.address.to_bytes(),
                &deposit.outpace_amount,
            )
            .await
            .expect("Should deposit with OUTPACE");

        // Counterfactual address deposit
        contracts
            .token
            .set_balance(
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
        ethereum::test_util::{GANACHE_1, GANACHE_1337, KEYSTORES},
        prelude::*,
        primitives::ChainOf,
        Adapter, Ethereum,
    };
    use chrono::Utc;
    use primitives::{
        balances::CheckedState,
        sentry::{
            campaign_create::CreateCampaign, AccountingResponse, Event, SuccessResponse, CLICK,
            IMPRESSION,
        },
        spender::Spender,
        test_util::{
            ADVERTISER, ADVERTISER_2, DUMMY_AD_UNITS, DUMMY_IPFS, GUARDIAN, GUARDIAN_2, IDS,
            PUBLISHER, PUBLISHER_2,
        },
        unified_num::FromWhole,
        util::{logging::new_logger, ApiUrl},
        validator::{ApproveState, Heartbeat, NewState, RejectState},
        Balances, BigNum, Campaign, CampaignId, Channel, ChannelId, UnifiedNum,
    };
    use reqwest::{Client, StatusCode};
    use slog::info;
    use validator_worker::{worker::Worker, SentryApi};

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
            campaign::{Active, Pricing, Validators},
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
            // fee per 1000 (pro mille) = 5.00000000 (UnifiedNum)
            // fee per 1 payout: payout * fee / 1000 = payout * 0.00500000
            fee: 500_000_000.into(),
            fee_addr: None,
        };

        let follower_desc = ValidatorDesc {
            id: VALIDATORS[&FOLLOWER].address.into(),
            url: VALIDATORS[&FOLLOWER].sentry_url.to_string(),
            // min_validator_fee for token: 0.000_010
            // fee per 1000 (pro mille) = 4.00000000 (UnifiedNum)
            // fee per 1 payout: payout * fee / 1000 = payout * 0.00400000
            fee: 400_000_000.into(),
            fee_addr: None,
        };

        let validators = Validators::new((leader_desc, follower_desc));

        Campaign {
            id: "0x936da01f9abd4d9d80c702af85c822a8"
                .parse()
                .expect("Should parse"),
            channel,
            creator: *ADVERTISER,
            // 150.00000000
            budget: UnifiedNum::from(15_000_000_000),
            validators,
            title: Some("Dummy Campaign".to_string()),
            pricing_bounds: vec![
                (
                    IMPRESSION,
                    Pricing {
                        // 0.04000000
                        // Per 1000 = 40.00000000
                        min: 4_000_000.into(),
                        // 0.05000000
                        // Per 1000 = 50.00000000
                        max: 5_000_000.into(),
                    },
                ),
                (
                    CLICK,
                    Pricing {
                        // 0.06000000
                        // Per 1000 = 60.00000000
                        min: 6_000_000.into(),
                        // 0.10000000
                        // Per 1000 = 100.00000000
                        max: 10_000_000.into(),
                    },
                ),
            ]
            .into_iter()
            .collect(),
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
            campaign::{Active, Pricing, Validators},
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
            // fee per 1 = 0.00010000
            fee: UnifiedNum::from_whole(0.1),
            fee_addr: None,
        };

        // Uses the VALIDATORS[&LEADER] as the Follower for this Channel
        // switches the URL as well
        let follower_desc = ValidatorDesc {
            id: VALIDATORS[&LEADER].address.into(),
            url: VALIDATORS[&LEADER].sentry_url.to_string(),
            // fee per 1000 (pro mille) = 0.05000000 (UnifiedNum)
            // fee per 1 = 0.00005000
            fee: UnifiedNum::from_whole(0.05),
            fee_addr: None,
        };

        let validators = Validators::new((leader_desc, follower_desc));

        // CAMPAIGN_2 budget 20 TOKENs (2_000_000_000)
        // leader fee (pro mile) 10_000_000 = 0.10000000 TOKENs
        // follower fee (pro mile) 5_000_000 = 0.05000000 TOKENs
        // IMPRESSION pricing (min) - 1 TOKEN
        // CLICK pricing (min) - 3 TOKENs
        //
        Campaign {
            id: "0x127b98248f4e4b73af409d10f62daeaa"
                .parse()
                .expect("Should parse"),
            channel,
            creator: *ADVERTISER,
            // 20.00000000
            budget: UnifiedNum::from_whole(20),
            validators,
            title: Some("Dummy Campaign 2 in Chain #1337".to_string()),
            pricing_bounds: vec![
                (
                    IMPRESSION,
                    Pricing {
                        // 1 TOKEN
                        min: UnifiedNum::from_whole(1),
                        // 2 TOKENs
                        max: UnifiedNum::from_whole(2),
                    },
                ),
                (
                    CLICK,
                    Pricing {
                        // 3 TOKENs
                        min: UnifiedNum::from_whole(3),
                        // 5 TOKENs
                        max: UnifiedNum::from_whole(5),
                    },
                ),
            ]
            .into_iter()
            .collect(),
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

    /// This Campaign has a token from the GANACHE_1 chain instead of the GANACHE_1337 one like the others
    static CAMPAIGN_3: Lazy<Campaign> = Lazy::new(|| {
        use chrono::TimeZone;
        use primitives::{
            campaign::{Active, Pricing, Validators},
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
            // fee per 1000 (pro mille) = 2.00000000
            // fee per 1 payout: payout * fee / 1000 = payout * 0.00200000
            fee: UnifiedNum::from_whole(2),
            fee_addr: None,
        };

        let follower_desc = ValidatorDesc {
            id: VALIDATORS[&FOLLOWER].address.into(),
            url: VALIDATORS[&FOLLOWER].sentry_url.to_string(),
            // min_validator_fee for token: 0.000_010
            // fee per 1000 (pro mille) = 1.75000000
            // fee per 1 payout: payout * fee / 1000 = payout * 0.00175000
            fee: UnifiedNum::from_whole(1.75),
            fee_addr: None,
        };

        let validators = Validators::new((leader_desc, follower_desc));

        Campaign {
            id: "0xa78f3492481b41a688488a7aa1ff17df"
                .parse()
                .expect("Should parse"),
            channel,
            creator: *ADVERTISER_2,
            // 20.00000000
            budget: UnifiedNum::from_whole(20),
            validators,
            title: Some("Dummy Campaign 3 in Chain #1".to_string()),
            pricing_bounds: vec![
                (
                    IMPRESSION,
                    Pricing {
                        // 0.01500000
                        // Per 1000 = 15.00000000
                        min: UnifiedNum::from_whole(0.015),
                        // 0.0250000
                        // Per 1000 = 25.00000000
                        max: UnifiedNum::from_whole(0.025),
                    },
                ),
                (
                    CLICK,
                    Pricing {
                        // 0.03500000
                        // Per 1000 = 35.00000000
                        min: UnifiedNum::from_whole(0.035),
                        // 0.06500000
                        // Per 1000 = 65.00000000
                        max: UnifiedNum::from_whole(0.065),
                    },
                ),
            ]
            .into_iter()
            .collect(),
            event_submission: Some(EventSubmission { allow: vec![] }),
            ad_units: vec![DUMMY_AD_UNITS[2].clone(), DUMMY_AD_UNITS[3].clone()],
            targeting_rules: Rules::new(),
            created: Utc.ymd(2021, 2, 1).and_hms(7, 0, 0),
            active: Active {
                to: Utc.ymd(2099, 1, 30).and_hms(0, 0, 0),
                from: None,
            },
        }
    });

    /// These `CAMPAIGN_2` events are used to test the `ApproveState` with `is_healthy: false`
    /// and `RejectState`
    /// 5 x IMPRESSIONs
    /// 4 x CLICKs
    static CAMPAIGN_2_EVENTS: Lazy<[Event; 8]> = Lazy::new(|| {
        [
            Event::Impression {
                publisher: *PUBLISHER_2,
                ad_unit: CAMPAIGN_2
                    .ad_units
                    .get(0)
                    .expect("Should exist in Campaign")
                    .ipfs,
                ad_slot: DUMMY_IPFS[3],
                referrer: Some("https://adex.network".into()),
            },
            Event::Impression {
                publisher: *PUBLISHER_2,
                ad_unit: CAMPAIGN_2
                    .ad_units
                    .get(0)
                    .expect("Should exist in Campaign")
                    .ipfs,
                ad_slot: DUMMY_IPFS[3],
                referrer: Some("https://adex.network".into()),
            },
            Event::Impression {
                publisher: *PUBLISHER_2,
                ad_unit: CAMPAIGN_2
                    .ad_units
                    .get(1)
                    .expect("Should exist in Campaign")
                    .ipfs,
                ad_slot: DUMMY_IPFS[3],
                referrer: Some("https://adex.network".into()),
            },
            Event::Impression {
                publisher: *PUBLISHER_2,
                ad_unit: CAMPAIGN_2
                    .ad_units
                    .get(1)
                    .expect("Should exist in Campaign")
                    .ipfs,
                ad_slot: DUMMY_IPFS[3],
                referrer: Some("https://adex.network".into()),
            },
            Event::Impression {
                publisher: *PUBLISHER_2,
                ad_unit: CAMPAIGN_2
                    .ad_units
                    .get(1)
                    .expect("Should exist in Campaign")
                    .ipfs,
                ad_slot: DUMMY_IPFS[3],
                referrer: Some("https://adex.network".into()),
            },
            Event::Click {
                publisher: *PUBLISHER_2,
                ad_unit: CAMPAIGN_2
                    .ad_units
                    .get(0)
                    .expect("Should exist in Campaign")
                    .ipfs,
                ad_slot: DUMMY_IPFS[3],
                referrer: Some("https://ambire.com".into()),
            },
            Event::Click {
                publisher: *PUBLISHER_2,
                ad_unit: CAMPAIGN_2
                    .ad_units
                    .get(1)
                    .expect("Should exist in Campaign")
                    .ipfs,
                ad_slot: DUMMY_IPFS[3],
                referrer: Some("https://ambire.com".into()),
            },
            Event::Click {
                publisher: *PUBLISHER_2,
                ad_unit: CAMPAIGN_2
                    .ad_units
                    .get(1)
                    .expect("Should exist in Campaign")
                    .ipfs,
                ad_slot: DUMMY_IPFS[3],
                referrer: Some("https://ambire.com".into()),
            },
        ]
    });

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn run_full_test() {
        let chain = GANACHE_1337.clone();
        assert_eq!(CAMPAIGN_1.channel.token, CAMPAIGN_2.channel.token);

        let token_chain_1337 = GANACHE_CONFIG
            .find_chain_of(CAMPAIGN_1.channel.token)
            .expect("Should find CAMPAIGN_1 channel token address in Config!");

        let token_chain_1 = GANACHE_CONFIG
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
                .expect("Should initialize ADVERTISER adapter"),
        )
        .unlock()
        .expect("Should unlock advertiser's Ethereum Adapter");

        // We use the Advertiser's `EthereumAdapter::get_auth` for authentication!
        let advertiser2_adapter = Adapter::new(
            Ethereum::init(KEYSTORES[&ADVERTISER_2].clone(), &GANACHE_CONFIG)
                .expect("Should initialize ADVERTISER_2 adapter"),
        )
        .unlock()
        .expect("Should unlock Advertiser 2 Ethereum Adapter");

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
        // Advertiser
        // Channel 1 in Chain #1337:
        // - Outpace: 150 TOKENs
        // - Counterfactual: 10 TOKENs
        //
        // Channel 2 in Chain #1337:
        // - Outpace: 30 TOKENs
        // - Counterfactual: 0 TOKENs
        //
        // Advertiser 2
        // Channel 3 in Chain #1:
        // - Outpace: 100 TOKENS
        // - Counterfactual: 20 TOKENs
        {
            let advertiser_deposits = [
                Deposit {
                    channel: CAMPAIGN_1.channel,
                    token: contracts_1337.token.info.clone(),
                    address: advertiser_adapter.whoami().to_address(),
                    outpace_amount: BigNum::with_precision(150, token_1337_precision),
                    counterfactual_amount: BigNum::with_precision(10, token_1337_precision),
                },
                Deposit {
                    channel: CAMPAIGN_2.channel,
                    token: contracts_1337.token.info.clone(),
                    address: advertiser_adapter.whoami().to_address(),
                    outpace_amount: BigNum::with_precision(30, token_1337_precision),
                    counterfactual_amount: BigNum::from(0),
                },
                Deposit {
                    channel: CAMPAIGN_3.channel,
                    token: contracts_1.token.info.clone(),
                    address: advertiser2_adapter.whoami().to_address(),
                    outpace_amount: BigNum::with_precision(100, token_1_precision),
                    counterfactual_amount: BigNum::with_precision(20, token_1_precision),
                },
            ];

            // 1st deposit
            // Chain #1337
            {
                setup
                    .deposit(&contracts_1337, &advertiser_deposits[0])
                    .await;

                // make sure we have the expected deposit returned from EthereumAdapter
                let eth_deposit = leader_adapter
                    .get_deposit(
                        &token_chain_1337.clone().with_channel(CAMPAIGN_1.channel),
                        advertiser_adapter.whoami().to_address(),
                    )
                    .await
                    .expect("Should get deposit for advertiser");

                assert_eq!(advertiser_deposits[0], eth_deposit);
            }

            // 2nd deposit
            // Chain #1337
            {
                setup
                    .deposit(&contracts_1337, &advertiser_deposits[1])
                    .await;

                // make sure we have the expected deposit returned from EthereumAdapter
                let eth_deposit = leader_adapter
                    .get_deposit(
                        &token_chain_1337.clone().with_channel(CAMPAIGN_2.channel),
                        advertiser_adapter.whoami().to_address(),
                    )
                    .await
                    .expect("Should get deposit for advertiser");

                assert_eq!(advertiser_deposits[1], eth_deposit);
            }

            // 3rd deposit
            // Chain #1
            {
                setup.deposit(&contracts_1, &advertiser_deposits[2]).await;

                // make sure we have the expected deposit returned from EthereumAdapter
                let eth_deposit = leader_adapter
                    .get_deposit(
                        &token_chain_1.clone().with_channel(CAMPAIGN_3.channel),
                        advertiser2_adapter.whoami().to_address(),
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
            // Deposit of Advertiser for Channel 1: 150 (outpace) + 10 (create2)
            // Campaign Budget: 400 TOKENs
            no_budget_campaign.budget = UnifiedNum::from(40_000_000_000);

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
                    // Expected: 160 TOKENs
                    total_deposited: UnifiedNum::from(16_000_000_000),
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
                info!(
                    setup.logger,
                    "CAMPAIGN_1 created in Leader ({:?})", CAMPAIGN_1.channel.leader
                );
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
                info!(
                    setup.logger,
                    "CAMPAIGN_1 created in Follower ({:?})", CAMPAIGN_1.channel.follower
                );
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
                info!(
                    setup.logger,
                    "CAMPAIGN_2 created in Leader ({:?})", CAMPAIGN_2.channel.leader
                );
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
                info!(
                    setup.logger,
                    "CAMPAIGN_2 created in Follower ({:?})", CAMPAIGN_2.channel.follower
                );
            }
        }

        // Create Campaign 3 w/ Channel 3 using Advertiser 2 on a different chain (Chain #1)
        // In Leader & Follower sentries
        // Response: 200 Ok
        // POST /v5/campaign
        {
            let second_chain = GANACHE_1.clone();
            let create_campaign_3 = CreateCampaign::from_campaign(CAMPAIGN_3.clone());

            assert_eq!(
                &token_chain_1.chain, &second_chain,
                "CAMPAIGN_3 should be using the #1 Chain which is setup in the Ganache Config"
            );

            {
                let leader_token = advertiser2_adapter
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
                info!(
                    setup.logger,
                    "CAMPAIGN_3 created in Leader ({:?})", CAMPAIGN_3.channel.leader
                );
            }

            {
                let follower_token = advertiser2_adapter
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
                info!(
                    setup.logger,
                    "CAMPAIGN_3 created in Follower ({:?})", CAMPAIGN_3.channel.follower
                );
            }
        }

        let leader_worker = Worker::from_sentry(leader_sentry.clone());
        let follower_worker = Worker::from_sentry(follower_sentry.clone());

        // Add new events for `CAMPAIGN_2` to sentry
        // Should trigger RejectedState on LEADER (the Channel's follower)
        //
        // Note: Follower and Leader for this channel are reversed!
        //
        // Channel 2 has only 1 spender with 50 deposit!
        // All spenders sum: 50.00000000
        //
        //
        // Channel Leader (FOLLOWER) has:
        // 5 IMPRESSIONS
        //
        // Channel Follower (LEADER) has:
        // 5 IMPRESSIONS
        // 3 CLICKS
        //
        // RejectState should be triggered by the Channel follower (LEADER) because:
        //
        // 5 x IMPRESSION = 5 TOKENs
        // 3 x CLICK =      9 TOKENs
        //                 ----------
        //                  14 TOKENs
        //
        // IMPRESSIONs:
        //
        // 5 x leader fee = 5 * (1 TOKENs * 0.1 (fee) / 1000 (pro mile) ) = 5 * 0.0001 TOKENs = 0.0005 TOKENs
        // 5 x follower fee = 5 * ( 1 TOKENs * 0.05 (fee) / 1000 (pro_mile) ) = 5 * 0.00005 TOKENs = 0.00025 TOKENs
        //
        // CLICKs (for 3):
        //
        // 3 x leader fee = 3 * ( 3 TOKENs * 0.1 (fee) / 1000 (pro mile) ) = 3 x 0.0003 TOKENs = 0.0009 TOKENs
        // 3 x follower fee = 3 x ( 3 TOKENs * 0.05 (fee) / 1000 (pro_mile) ) = 3 x 0.00015 TOKENs = 0.00045 TOKENs
        //
        // All payouts (for 5 IMPRESSIONs + 3 CLICKs):
        // Advertiser (spender) = 14.0021 TOKENs
        // Publisher (earner) = 14 TOKENs
        // Leader (earner) = 0.0005 (IMPRESSIONs) + 0.0009 (CLICKs) = 0.0014 TOKENs
        // Follower (earner) = 0.00025 (IMPRESSIONs) + 0.00045 (CLICKs) = 0.00070 TOKENs
        //
        // Total: 14.0021 TOKENs
        //
        // For Channel Follower (LEADER) which has all 5 IMPRESSIONs and 3/4 CLICKs:
        //
        // sum_our = 14.0021
        // sum_approved_mins (5 x IMPRESSION) = 5 + 0.0005 + 0.00025 = 5.00075
        // sum_approved_mins (3 x CLICKS) = 9 + 0.0009 + 0.00045 = 9.00135
        //
        // For Channel Leader (FOLLOWER) which has only 5 x IMPRESSION events
        //
        // 5 x IMPRESSIONs (only)
        // diff = 14.0021 - 5.00075 = 9.00135
        // health_penalty = 9.00135 * 1 000 / 30.0 = 300.045
        //
        // health = 1 000 - health_penalty = 699.955 (Unsignable)
        //
        // Ganache health_threshold_promilles = 950
        // Ganache health_unsignable_promilles = 750
        {
            // Take all 5 IMPRESSIONs and None of the CLICKs to trigger `RejectState` on the Follower of the Channel (LEADER)
            let channel_leader_events = &CAMPAIGN_2_EVENTS[..=4];
            // follower should receive all IMPRESSIONs & 3/3 CLICKs so it will trigger `RejectState`
            let channel_follower_events = CAMPAIGN_2_EVENTS.as_ref();

            let channel_leader_response = post_new_events(
                &follower_sentry,
                token_chain_1337.clone().with(CAMPAIGN_2.id),
                // the Leader of this channel is FOLLOWER!
                &channel_leader_events,
            )
            .await
            .expect("Posted events");

            assert_eq!(SuccessResponse { success: true }, channel_leader_response);

            let channel_follower_response = post_new_events(
                &leader_sentry,
                token_chain_1337.clone().with(CAMPAIGN_2.id),
                // the Follower of this channel is LEADER!
                channel_follower_events,
            )
            .await
            .expect("Posted events");

            assert_eq!(SuccessResponse { success: true }, channel_follower_response);

            info!(
                setup.logger,
                "Successful POST of events for CAMPAIGN_2 {:?} and Channel {:?} to Leader & Follower to trigger RejectState",
                CAMPAIGN_2.id,
                CAMPAIGN_2.channel.id()
            );

            // Channel 2 expected Accounting on Leader (FOLLOWER)
            {
                let expected_accounting = AccountingResponse {
                    balances: {
                        let mut balances = Balances::<CheckedState>::new();
                        // publisher (PUBLISHER_2) payout = 5.0
                        balances
                            .spend(*ADVERTISER, *PUBLISHER_2, UnifiedNum::from_whole(5))
                            .expect("Should not overflow");
                        // leader (FOLLOWER) payout = 0.0005
                        balances
                            .spend(*ADVERTISER, *FOLLOWER, UnifiedNum::from_whole(0.0005))
                            .expect("Should not overflow");
                        // follower (LEADER) payout = 0.00025
                        balances
                            .spend(*ADVERTISER, *LEADER, UnifiedNum::from_whole(0.00025))
                            .expect("Should not overflow");

                        balances
                    },
                };
                // Channel Leader (FOLLOWER)
                let actual_accounting = follower_sentry
                    .get_accounting(&token_chain_1337.clone().with_channel(CAMPAIGN_2.channel))
                    .await
                    .expect("Should get Channel Accounting");

                assert_eq!(expected_accounting, actual_accounting);
                info!(setup.logger, "Channel 1 {:?} has empty Accounting because no events have been submitted to any Campaign", CAMPAIGN_1.channel.id());
            }
        }

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
            info!(setup.logger, "Channel 1 {:?} has empty Accounting because no events have been submitted to any Campaign", CAMPAIGN_1.channel.id());
        }

        // For CAMPAIGN_2 & Channel 2
        //
        // Check NewState on Leader (FOLLOWER)
        // RejectState should not be generated by Follower (LEADER) because tick has ran before the Leader's (FOLLOWER)
        //
        // Leader (FOLLOWER) events:
        // - 5 IMPRESSIONs
        //
        // event payout * fee / 1000 (pro mile)
        //
        // 5 x leader fee = 5 * ( 1 * 0.1 / 1000 ) = 5 * 0.0001 = 0.0005
        // 5 x follower fee = 5 * ( 1 * 0.05 / 1000 ) = 5 * 0.00005 = 0.00025
        //
        // Payouts for 5 IMPRESSIONs:
        //
        // Advertiser (spender) = 5.00075
        // Publisher (earner) = 5
        // Leader (earner) = 0.0005
        // Follower (earner) = 0.00025
        //
        // Total: 5.00075 TOKENs
        {
            let latest_new_state_leader = follower_sentry
                .get_our_latest_msg(CAMPAIGN_2.channel.id(), &["NewState"])
                .await
                .expect("Should fetch NewState from Channel's Leader (Who am I) in FOLLOWER sentry")
                .map(|message| {
                    NewState::<CheckedState>::try_from(message)
                        .expect("Should be NewState with Checked Balances")
                })
                .expect("Should have a NewState in Channel's Leader for the Campaign 2 channel");

            let latest_reject_state_follower = leader_sentry
                .get_our_latest_msg(CAMPAIGN_2.channel.id(), &["RejectState"])
                .await
                .expect(
                    "Should successfully try to fetch a RejectState from Channel's Follower (Who am I) in LEADER sentry",
                );

            let expected_new_state_balances = {
                let mut balances = Balances::<CheckedState>::new();
                let multiplier = 10_u64.pow(UnifiedNum::PRECISION.into());
                // Channel's Leader (FOLLOWER) & Follower (LEADER) are reversed!
                balances
                    .spend(*ADVERTISER, *PUBLISHER_2, UnifiedNum::from(5 * multiplier))
                    .expect("Should not overflow");
                // total Leader fee: 0.0005 TOKENs
                balances
                    .spend(*ADVERTISER, *FOLLOWER, 50_000_u64.into())
                    .expect("Should not overflow");
                // total Follower fee: 0.00025 TOKENs
                balances
                    .spend(*ADVERTISER, *LEADER, 25_000_u64.into())
                    .expect("Should not overflow");

                balances
            };

            pretty_assertions::assert_eq!(
                expected_new_state_balances,
                latest_new_state_leader.balances,
                "Expected Channel's Leader (FOLLOWER) balances should match"
            );

            assert_eq!(
                None, latest_reject_state_follower,
                "Channel's follower should not have RejectState yet"
            );
        }

        // All Channels should have a heartbeat message now
        // Channel 1
        // Channel 2
        // Channel 3
        {
            // Channel 1
            let _channel_1_heartbeat = leader_sentry
                .get_our_latest_msg(CAMPAIGN_1.channel.id(), &["Heartbeat"])
                .await
                .expect("Should fetch Heartbeat from Leader")
                .map(|message| Heartbeat::try_from(message).expect("Should be Heartbeat"))
                .expect("Should have a Heartbeat in Leader for the Campaign 1 channel");

            // Channel 2
            let _channel_2_heartbeat = leader_sentry
                .get_our_latest_msg(CAMPAIGN_2.channel.id(), &["Heartbeat"])
                .await
                .expect("Should fetch Heartbeat from Leader")
                .map(|message| Heartbeat::try_from(message).expect("Should be Heartbeat"))
                .expect("Should have a Heartbeat in Leader for the Campaign 2 channel");

            // Channel 3
            let _channel_3_heartbeat = leader_sentry
                .get_our_latest_msg(CAMPAIGN_3.channel.id(), &["Heartbeat"])
                .await
                .expect("Should fetch Heartbeat from Leader")
                .map(|message| Heartbeat::try_from(message).expect("Should be Heartbeat"))
                .expect("Should have a Heartbeat in Leader for the Campaign 3 channel");
        }

        // Add new events for `CAMPAIGN_1` to sentry
        {
            let events = vec![
                Event::Impression {
                    publisher: *PUBLISHER,
                    ad_unit: CAMPAIGN_1
                        .ad_units
                        .get(0)
                        .expect("Should exist in Campaign")
                        .ipfs,
                    ad_slot: DUMMY_IPFS[2],
                    referrer: Some("https://adex.network".into()),
                },
                Event::Click {
                    publisher: *PUBLISHER,
                    ad_unit: CAMPAIGN_1
                        .ad_units
                        .get(0)
                        .expect("Should exist in Campaign")
                        .ipfs,
                    ad_slot: DUMMY_IPFS[2],
                    referrer: Some("https://ambire.com".into()),
                },
            ];

            let leader_response = post_new_events(
                &leader_sentry,
                token_chain_1337.clone().with(CAMPAIGN_1.id),
                &events,
            )
            .await
            .expect("Posted events");

            assert_eq!(SuccessResponse { success: true }, leader_response);

            let follower_response = post_new_events(
                &follower_sentry,
                token_chain_1337.clone().with(CAMPAIGN_1.id),
                &events,
            )
            .await
            .expect("Posted events");

            assert_eq!(SuccessResponse { success: true }, follower_response);
            info!(
                setup.logger,
                "Successful POST of events for CAMPAIGN_1 {:?} and Channel {:?} to Leader & Follower",
                CAMPAIGN_1.id,
                CAMPAIGN_1.channel.id()
            );
        }

        // Add new events for `CAMPAIGN_3` to sentry
        // 2 IMPRESSIONS
        // 2 CLICKS
        {
            let events = vec![
                Event::Impression {
                    publisher: *PUBLISHER_2,
                    ad_unit: CAMPAIGN_3
                        .ad_units
                        .get(1)
                        .expect("Should exist in Campaign")
                        .ipfs,
                    ad_slot: DUMMY_IPFS[3],
                    referrer: Some("https://adex.network".into()),
                },
                Event::Impression {
                    publisher: *PUBLISHER_2,
                    ad_unit: CAMPAIGN_3
                        .ad_units
                        .get(1)
                        .expect("Should exist in Campaign")
                        .ipfs,
                    ad_slot: DUMMY_IPFS[3],
                    referrer: Some("https://adex.network".into()),
                },
                Event::Click {
                    publisher: *PUBLISHER_2,
                    ad_unit: CAMPAIGN_3
                        .ad_units
                        .get(1)
                        .expect("Should exist in Campaign")
                        .ipfs,
                    ad_slot: DUMMY_IPFS[3],
                    referrer: Some("https://ambire.com".into()),
                },
                Event::Click {
                    publisher: *PUBLISHER_2,
                    ad_unit: CAMPAIGN_3
                        .ad_units
                        .get(1)
                        .expect("Should exist in Campaign")
                        .ipfs,
                    ad_slot: DUMMY_IPFS[3],
                    referrer: Some("https://ambire.com".into()),
                },
            ];

            let response_leader = post_new_events(
                &leader_sentry,
                token_chain_1.clone().with(CAMPAIGN_3.id),
                &events,
            )
            .await
            .expect("Posted events");

            assert_eq!(SuccessResponse { success: true }, response_leader);

            let follower_response = post_new_events(
                &follower_sentry,
                token_chain_1.clone().with(CAMPAIGN_3.id),
                &events,
            )
            .await
            .expect("Posted events");

            assert_eq!(SuccessResponse { success: true }, follower_response);
            info!(
                setup.logger,
                "Successful POST of events for CAMPAIGN_3 {:?} and Channel {:?} to Leader & Follower",
                CAMPAIGN_3.id,
                CAMPAIGN_3.channel.id()
            );
        }

        // CAMPAIGN_1 & Channel 1 expected Accounting
        //
        // Fees are calculated based on pro mile of the payout
        // event payout * fee / 1000
        //
        // leader fee (per 1000): 5
        // follower fee (per 1000): 4
        // IMPRESSION price (min): 0.04
        // CLICK price (min): 0.06
        //
        // 1 x IMPRESSION:
        // - Publisher payout: 0.04 = UnifiedNum(4 000 000)
        // - Leader fees: 0.04 * 5 / 1 000 = 0.0002 = UnifiedNum(20 000)
        // - Follower fees: 0.04 * 4 / 1 000 = 0.00016 = UnifiedNum(16 000)
        //
        // 1 x CLICK:
        // - Publisher payout: 0.06 = UnifiedNum(6 000 000)
        // - Leader fees: 0.06 * 5 / 1 000 = 0.0003 = UnifiedNum(30 000)
        // - Follower fees: 0.06 * 4 / 1 000 = 0.00024 = UnifiedNum(24 000)
        //
        // Creator (Advertiser) pays out:
        //
        // Publisher total payout: 0.04 (impression) + 0.06 (click) = 0.1 = UnifiedNum(10 000 000)
        // Leader total fees: 0.0002 + 0.0003 = 0.0005 = UnifiedNum (50 000)
        // Follower total fees: 0.00016 + 0.00024 = 0.0004 = UnifiedNum(40 000)
        //
        // events_payout + leader fee + follower fee
        // 0.1 + (0.0002 + 0.0003) + (0.00016 + 0.00024) = 0.1009 = UnifiedNum(10 090 000)
        {
            let mut expected_balances = Balances::new();

            expected_balances
                .spend(
                    CAMPAIGN_1.creator,
                    CAMPAIGN_1.channel.leader.to_address(),
                    UnifiedNum::from_whole(0.0005),
                )
                .expect("Should spend for Leader");
            expected_balances
                .spend(
                    CAMPAIGN_1.creator,
                    CAMPAIGN_1.channel.follower.to_address(),
                    UnifiedNum::from_whole(0.0004),
                )
                .expect("Should spend for Follower");
            expected_balances
                .spend(CAMPAIGN_1.creator, *PUBLISHER, UnifiedNum::from_whole(0.1))
                .expect("Should spend for Publisher");

            let expected_accounting = AccountingResponse {
                balances: expected_balances,
            };

            let actual_accounting = leader_sentry
                .get_accounting(&token_chain_1337.clone().with_channel(CAMPAIGN_1.channel))
                .await
                .expect("Should get Channel Accounting");

            pretty_assertions::assert_eq!(expected_accounting, actual_accounting);
            info!(setup.logger, "Successfully validated Accounting Balances for Channel 1 {:?} after CAMPAIGN_1 events {:?}", CAMPAIGN_1.channel.id(), CAMPAIGN_1.id);
        }

        // CAMPAIGN_3 & Channel 3 expected Accounting
        // Fees are calculated based on pro mile of the payout
        // event payout * fee / 1000
        //
        // leader fee (per 1000): 2
        // follower fee (per 1000): 1.75
        // IMPRESSION price (min): 0.015
        // CLICK price (min): 0.035
        //
        // 2 x IMPRESSION:
        // - Publisher2 payout: 2 * 0.015 = = 0.030 UnifiedNum(3 000 000)
        // - Leader fees: 2 * (0.015 * 2 / 1000) = 0.00006 = UnifiedNum(6 000)
        // - Follower fees: 2 * (0.015 * 1.75 / 1000) = 0.0000525 = UnifiedNum(5 250)
        //
        // 2 x CLICK:
        // - Publisher2 payout: 2 * 0.035 = UnifiedNum(7 000 000)
        // - Leader fees: 2 * (0.035 * 2 / 1000) = 0.00014 = UnifiedNum(14 000)
        // - Follower fees: 2 * (0.035 * 1.75 / 1000) = 0.0001225 = UnifiedNum(12 250)
        //
        // Creator (Advertiser2) pays out:
        //
        // Publisher2 total payout: 2 * 0.015 (impression) + 2 * 0.035 (click) = 0.1 = UnifiedNum(10 000 000)
        // Leader total fees: 0.00006 + 0.00014 = 0.00020 = UnifiedNum(20 000)
        // Follower total fees: 0.0000525 + 0.0001225 = 0.000175 = UnifiedNum(17 500)
        //
        // events_payout + leader fee + follower fee
        // 0.1 + 0.00020 + 0.000175 = 0.100375 = UnifiedNum(10 037 500)
        {
            let mut expected_balances = Balances::new();

            expected_balances
                .spend(
                    CAMPAIGN_3.creator,
                    CAMPAIGN_3.channel.leader.to_address(),
                    UnifiedNum::from_whole(0.00020),
                )
                .expect("Should spend for Leader");
            expected_balances
                .spend(
                    CAMPAIGN_3.creator,
                    CAMPAIGN_3.channel.follower.to_address(),
                    UnifiedNum::from_whole(0.000175),
                )
                .expect("Should spend for Follower");
            expected_balances
                .spend(
                    CAMPAIGN_3.creator,
                    *PUBLISHER_2,
                    UnifiedNum::from_whole(0.1),
                )
                .expect("Should spend for Publisher");

            let expected_accounting = AccountingResponse {
                balances: expected_balances,
            };

            let actual_accounting = leader_sentry
                .get_accounting(&token_chain_1.with_channel(CAMPAIGN_3.channel))
                .await
                .expect("Should get Channel Accounting");

            pretty_assertions::assert_eq!(expected_accounting, actual_accounting);
            info!(setup.logger, "Successfully validated Accounting Balances for Channel 3 {:?} after CAMPAIGN_3 events {:?}", CAMPAIGN_3.channel.id(), CAMPAIGN_3.id);
        }

        // leader single worker tick
        leader_worker.all_channels_tick().await;
        // follower single worker tick
        follower_worker.all_channels_tick().await;

        // For CAMPAIGN_1
        //
        // Check NewState existence of Channel 1 after the validator ticks
        // For both Leader & Follower
        // Assert that both states are the same!
        //
        // Check ApproveState of the Follower
        // Assert that it exists in both validators
        {
            let latest_new_state_leader = leader_sentry
                .get_our_latest_msg(CAMPAIGN_1.channel.id(), &["NewState"])
                .await
                .expect("Should fetch NewState from Leader (Who am I) in Leader sentry")
                .map(|message| {
                    NewState::<CheckedState>::try_from(message)
                        .expect("Should be NewState with Checked Balances")
                })
                .expect("Should have a NewState in Leader for the Campaign 1 channel");

            // Check balances in Leader's NewState
            {
                let mut expected_balances = Balances::new();
                expected_balances
                    .spend(
                        CAMPAIGN_1.creator,
                        CAMPAIGN_1.channel.leader.to_address(),
                        UnifiedNum::from_u64(50_000),
                    )
                    .expect("Should spend");
                expected_balances
                    .spend(
                        CAMPAIGN_1.creator,
                        CAMPAIGN_1.channel.follower.to_address(),
                        UnifiedNum::from_u64(40_000),
                    )
                    .expect("Should spend");
                expected_balances
                    .spend(
                        CAMPAIGN_1.creator,
                        *PUBLISHER,
                        UnifiedNum::from_u64(10_000_000),
                    )
                    .expect("Should spend");

                pretty_assertions::assert_eq!(
                    latest_new_state_leader.balances,
                    expected_balances,
                    "Balances are as expected"
                );
            }

            let last_approved_response_follower = follower_sentry
                .get_last_approved(CAMPAIGN_1.channel.id())
                .await
                .expect("Should fetch Approve state from Follower");

            let last_approved_response_leader = leader_sentry
                .get_last_approved(CAMPAIGN_1.channel.id())
                .await
                .expect("Should fetch Approve state from Leader");

            // Due to timestamp differences in the `received` field
            // we can only `assert_eq!` the messages themselves
            pretty_assertions::assert_eq!(
                last_approved_response_leader
                    .heartbeats
                    .expect("Leader response should have heartbeats")
                    .clone()
                    .into_iter()
                    .map(|message| message.msg)
                    .collect::<Vec<_>>(),
                last_approved_response_follower
                    .heartbeats
                    .expect("Follower response should have heartbeats")
                    .clone()
                    .into_iter()
                    .map(|message| message.msg)
                    .collect::<Vec<_>>(),
                "Leader and Follower should both have the same last Approved response"
            );

            let last_approved_follower = last_approved_response_follower
                .last_approved
                .expect("Should have last approved messages for the events we've submitted");

            let last_approved_leader = last_approved_response_leader
                .last_approved
                .expect("Should have last approved messages for the events we've submitted");

            // Due to the received time that can be different in messages
            // we must check the actual ValidatorMessage without the timestamps
            {
                let msg_new_state_leader = last_approved_leader
                    .new_state
                    .expect("Leader should have last approved NewState");

                assert_eq!(
                    msg_new_state_leader.from, IDS[&LEADER],
                    "NewState should be received from Leader"
                );

                let msg_approve_state_leader = last_approved_leader
                    .approve_state
                    .expect("Leader should have last approved ApproveState");

                assert_eq!(
                    msg_approve_state_leader.from, IDS[&FOLLOWER],
                    "ApproveState should be received from Follower"
                );

                let msg_new_state_follower = last_approved_follower
                    .new_state
                    .expect("Follower should have last approved NewState");

                assert_eq!(
                    msg_new_state_follower.from, IDS[&LEADER],
                    "NewState should be received from Leader"
                );

                let msg_approve_state_follower = last_approved_follower
                    .approve_state
                    .expect("Follower should have last approved ApproveState");

                assert_eq!(
                    msg_approve_state_follower.from, IDS[&FOLLOWER],
                    "ApproveState should be received from Follower"
                );

                let new_state_leader = msg_new_state_leader
                    .msg
                    .clone()
                    .into_inner()
                    .try_checked()
                    .expect("NewState should have valid CheckedState Balances");

                let new_state_follower = msg_new_state_follower
                    .msg
                    .clone()
                    .into_inner()
                    .try_checked()
                    .expect("NewState should have valid CheckedState Balances");

                assert_eq!(
                    new_state_leader, new_state_follower,
                    "Last approved NewState in Leader & Follower should be the same"
                );

                pretty_assertions::assert_eq!(
                    latest_new_state_leader,
                    new_state_leader,
                    "Latest NewState from Leader should be the same as last approved NewState from Leader & Follower"
                );
            }
        }

        // For CAMPAIGN_2 & Channel 2
        //
        // Check RejectState on Follower (LEADER) in Channel 2 after validator tick
        //
        // For Channel Follower (LEADER) which has all events:
        //
        // All payouts (for 5 IMPRESSIONs + 3/4 CLICKs):
        // Advertiser (spender) = 14.0021 TOKENs
        // Publisher (earner) = 14 TOKENs
        // Leader (earner) = 0.0005 (IMPRESSIONs) + 0.0009 (CLICKs) = 0.0014 TOKENs
        // Follower (earner) = 0.00025 (IMPRESSIONs) + 0.00045 (CLICKs) = 0.00070 TOKENs
        //
        // Total: 14.0021 TOKENs
        //
        // sum_our = 14.0021
        // sum_approved_mins (5 x IMPRESSION) = 5 + 0.0005 + 0.00025 = 5.00075
        // sum_approved_mins (3 x CLICKS) = 9 + 0.0009 + 0.00045 = 9.00135
        //
        // For Channel Leader (FOLLOWER) which has only the 5 IMPRESSION events
        //
        // 5 x IMPRESSIONs (only)
        // diff = 14.0021 - 5.00075 = 9.00135
        // health_penalty = 9.00135 * 1 000 / 30.0 = 300.045
        // health = 1 000 - health_penalty = 699.955 (Unsignable)
        //
        // Ganache health_threshold_promilles = 950
        // Ganache health_unsignable_promilles = 750
        {
            let latest_reject_state_follower = leader_sentry
                .get_our_latest_msg(CAMPAIGN_2.channel.id(), &["RejectState"])
                .await
                .expect(
                    "Should fetch RejectState from Channel's Follower (Who am I) in Leader sentry",
                )
                .map(|message| {
                    RejectState::<CheckedState>::try_from(message)
                        .expect("Should be RejectState with valid Checked Balances")
                })
                .expect(
                    "Should have a RejectState in Channel's Follower for the Campaign 2 channel",
                );

            let rejected_balances = latest_reject_state_follower
                .balances
                .expect("Channel Follower (LEADER) should have RejectState with balances");
            // Expected Leader (FOLLOWER) total rejected in Follower: 5.00075
            assert_eq!(
                UnifiedNum::from_whole(5.00075),
                rejected_balances
                    .sum()
                    .expect("Should not overflow summing balances")
                    // does not really matter if we're checking earners or spenders for CheckedState
                    .0
            )
        }

        // For CAMPAIGN_3
        //
        // Check NewState existence of Channel 3 after the validator ticks
        // For both Leader & Follower
        // Assert that both states are the same!
        //
        // Check ApproveState of the Follower
        // Assert that it exists in both validators
        {
            let latest_new_state_leader = leader_sentry
                .get_our_latest_msg(CAMPAIGN_3.channel.id(), &["NewState"])
                .await
                .expect("Should fetch NewState from Leader (Who am I) in Leader sentry")
                .map(|message| {
                    NewState::<CheckedState>::try_from(message)
                        .expect("Should be NewState with Checked Balances")
                })
                .expect("Should have a NewState in Leader for the Campaign 3 channel");

            // Check balances in Leader's NewState
            {
                let mut expected_balances = Balances::new();
                expected_balances
                    .spend(
                        CAMPAIGN_3.creator,
                        CAMPAIGN_3.channel.leader.to_address(),
                        UnifiedNum::from_whole(0.0002),
                    )
                    .expect("Should spend");
                expected_balances
                    .spend(
                        CAMPAIGN_3.creator,
                        CAMPAIGN_3.channel.follower.to_address(),
                        UnifiedNum::from_whole(0.000175),
                    )
                    .expect("Should spend");
                expected_balances
                    .spend(
                        CAMPAIGN_3.creator,
                        *PUBLISHER_2,
                        UnifiedNum::from_whole(0.1),
                    )
                    .expect("Should spend");

                pretty_assertions::assert_eq!(
                    latest_new_state_leader.balances,
                    expected_balances,
                    "Balances are as expected"
                );
            }

            let last_approved_response_follower = follower_sentry
                .get_last_approved(CAMPAIGN_3.channel.id())
                .await
                .expect("Should fetch Approve state from Follower");

            let last_approved_response_leader = leader_sentry
                .get_last_approved(CAMPAIGN_3.channel.id())
                .await
                .expect("Should fetch Approve state from Leader");

            // Due to timestamp differences in the `received` field
            // we can only `assert_eq!` the messages themselves
            pretty_assertions::assert_eq!(
                last_approved_response_leader
                    .heartbeats
                    .expect("Leader response should have heartbeats")
                    .clone()
                    .into_iter()
                    .map(|message| message.msg)
                    .collect::<Vec<_>>(),
                last_approved_response_follower
                    .heartbeats
                    .expect("Follower response should have heartbeats")
                    .clone()
                    .into_iter()
                    .map(|message| message.msg)
                    .collect::<Vec<_>>(),
                "Leader and Follower should both have the same last Approved response"
            );

            let last_approved_follower = last_approved_response_follower
                .last_approved
                .expect("Should have last approved messages for the events we've submitted");

            let last_approved_leader = last_approved_response_leader
                .last_approved
                .expect("Should have last approved messages for the events we've submitted");

            // Due to the received time that can be different in messages
            // we must check the actual ValidatorMessage without the timestamps
            {
                let msg_new_state_leader = last_approved_leader
                    .new_state
                    .expect("Leader should have last approved NewState");

                assert_eq!(
                    msg_new_state_leader.from, IDS[&LEADER],
                    "NewState should be received from Leader"
                );

                let msg_approve_state_leader = last_approved_leader
                    .approve_state
                    .expect("Leader should have last approved ApproveState");

                assert_eq!(
                    msg_approve_state_leader.from, IDS[&FOLLOWER],
                    "ApproveState should be received from Follower"
                );

                let msg_new_state_follower = last_approved_follower
                    .new_state
                    .expect("Follower should have last approved NewState");

                assert_eq!(
                    msg_new_state_follower.from, IDS[&LEADER],
                    "NewState should be received from Leader"
                );

                let msg_approve_state_follower = last_approved_follower
                    .approve_state
                    .expect("Follower should have last approved ApproveState");

                assert_eq!(
                    msg_approve_state_follower.from, IDS[&FOLLOWER],
                    "ApproveState should be received from Follower"
                );

                let new_state_leader = msg_new_state_leader
                    .msg
                    .clone()
                    .into_inner()
                    .try_checked()
                    .expect("NewState should have valid CheckedState Balances");

                let new_state_follower = msg_new_state_follower
                    .msg
                    .clone()
                    .into_inner()
                    .try_checked()
                    .expect("NewState should have valid CheckedState Balances");

                assert_eq!(
                    new_state_leader, new_state_follower,
                    "Last approved NewState in Leader & Follower should be the same"
                );

                pretty_assertions::assert_eq!(
                    latest_new_state_leader,
                    new_state_leader,
                    "Latest NewState from Leader should be the same as last approved NewState from Leader & Follower"
                );
            }
        }

        // For CAMPAIGN_2 & Channel 2
        // Trigger Unhealthy but signable NewState with 5 IMPRESSIONs & 2 CLICKs in Leader
        // As opposed to 5 IMPRESSIONs & 3 CLICKs in Follower
        //
        // 5 x IMPRESSION = 5 TOKENs
        // 3 x CLICK =      9 TOKENs
        //
        //                  14 TOKENs
        //
        // IMPRESSIONs:
        //
        // 5 x leader fee = 5 * (1 TOKENs * 0.1 (fee) / 1000 (pro mile) ) = 5 x 0.0001 TOKENs = 0.0005 TOKENs
        // 5 x follower fee = 5 x ( 1 TOKENs * 0.05 (fee) / 1000 (pro_mile) ) = 5 x 0.00005 TOKENs = 0.00025 TOKENs
        //
        // CLICKs (for 3):
        //
        // 3 x leader fee = 3 * (3 TOKENs * 0.1 (fee) / 1000 (pro mile) ) = 3 x 0.0003 TOKENs = 0.0009 TOKENs
        // 3 x follower fee = 3 x ( 3 TOKENs * 0.05 (fee) / 1000 (pro_mile) ) = 3 x 0.00015 TOKENs = 0.00045 TOKENs
        //
        // CLICKS (for 2):
        // 2 x leader fee = 2 * (3 TOKENs * 0.1 (fee) / 1000 (pro mile) ) = 2 x 0.0003 TOKENs = 0.0006 TOKENs
        // 2 x follower fee = 2 x ( 3 TOKENs * 0.05 (fee) / 1000 (pro_mile) ) = 2 x 0.00015 TOKENs = 0.00030 TOKENs
        //
        // Payouts (all current Follower events: 5 IMPRESSIONs + 3 CLICKs):
        // Advertiser (spender) = 14.0021
        // Publisher (earner) = 14 TOKENs
        // Leader (earner) = 0.0005 (IMPRESSIONs) + 0.0009 (CLICKs) = 0.0014 TOKENs
        // Follower (earner) = 0.00025 (IMPRESSIONs) + 0.00045 (CLICKs) = 0.00070 TOKENs
        //
        // Total: 14.0021
        //
        // sum_our = 14.0021
        // sum_approved_mins (5 x IMPRESSION) = 5 + 0.0005 + 0.00025 = 5.00075
        // sum_approved_mins (2 x CLICKS) = 6 + 0.0006 + 0.00030 = 6.0009
        //                                                       ------------
        //                                                          11.00165
        //
        // 5 IMPRESSIONs + 2 CLICKs
        //
        // diff = 14.0021 - (5.00075 + 6.0009) = 3.00045
        // health_penalty = 3.00045 * 1 000 / 30.0 = 100.015
        //
        // health = 1 000 - health_penalty = 899.985 (Unhealthy but Signable)
        //
        // Ganache health_threshold_promilles = 950
        // Ganache health_unsignable_promilles = 750
        //
        // Add new events for `CAMPAIGN_2` to sentry Follower
        // Should trigger RejectedState
        //
        // Note: Follower and Leader for this channel are reversed!
        //
        // Channel's Leader (FOLLOWER) events opposed to Channel's Follower (LEADER) events:
        //
        // 5 IMPRESSIONs
        // 2 (out of 3) CLICKs
        {
            // Prepare 2 out of 4 CLICK events to trigger unhealthy `ApproveState` on the Follower of the Channel (LEADER)
            let channel_leader_events = &CAMPAIGN_2_EVENTS[5..=6];

            let channel_leader_response = post_new_events(
                // the Leader of this channel is FOLLOWER!
                &follower_sentry,
                token_chain_1337.clone().with(CAMPAIGN_2.id),
                &channel_leader_events,
            )
            .await
            .expect("Posted events");

            assert_eq!(SuccessResponse { success: true }, channel_leader_response);

            info!(
                setup.logger,
                "Successful POST of 2/3 CLICK events found in the Channel's Follower for CAMPAIGN_2 {:?} and Channel {:?} to Channel Leader to trigger unhealthy ApproveState",
                CAMPAIGN_2.id,
                CAMPAIGN_2.channel.id()
            );
        }

        // IMPORTANT! Call the FOLLOWER tick first as it's the Channel 2 Leader!
        // This will trigger a the new state and the Channel 2 follower (LEADER) will process that.

        // follower single worker tick
        follower_worker.all_channels_tick().await;
        // leader single worker tick
        leader_worker.all_channels_tick().await;

        {
            let latest_approve_state_follower = leader_sentry
                .get_our_latest_msg(CAMPAIGN_2.channel.id(), &["ApproveState"])
                .await
                .expect(
                    "Should fetch ApproveState from Channel's Follower (Who am I) from LEADER sentry",
                )
                .map(|message| {
                    ApproveState::try_from(message)
                        .expect("Should be ApproveState with valid Checked Balances")
                })
                .expect("Should have a ApproveState in Channel's Follower for the Campaign 2 channel");

            let latest_new_state_leader = follower_sentry
                .get_our_latest_msg(CAMPAIGN_2.channel.id(), &["NewState"])
                .await
                .expect(
                    "Should fetch NewState from Channel's Leader (Who am I) from FOLLOWER sentry",
                )
                .map(|message| {
                    NewState::<CheckedState>::try_from(message)
                        .expect("Should be NewState with valid Checked Balances")
                })
                .expect("Should have a NewState in Channel's Follower for the Campaign 2 channel");

            assert_eq!(
                latest_approve_state_follower.state_root, latest_new_state_leader.state_root,
                "Latest ApproveState in Follower should correspond to the latest NewState Leader"
            );
            assert!(!latest_approve_state_follower.is_healthy);
            assert_eq!(
                UnifiedNum::from_whole(11.00165),
                latest_new_state_leader
                    .balances
                    .sum()
                    .expect("Should not overflow summing balances")
                    // does not really matter if we're checking earners or spenders for CheckedState
                    .0
            )
        }

        // For CAMPAIGN_2 & Channel 2
        //
        // Post new events to Channel Leader (FOLLOWER)
        //
        // Trigger a healthy ApproveState by posting the last CLICK event in Leader (FOLLOWER)
        // this will create a healthy NewState and have the exact same number of events as the Follower (LEADER)
        //
        // 5 x IMPRESSION = 5 TOKENs
        // 3 x CLICK =      9 TOKENs
        //
        //                  14 TOKENs
        //
        // Channel's Leader (FOLLOWER) events opposed to Channel's Follower (LEADER) events:
        //
        // 5 IMPRESSIONs
        // 3 (out of 3) CLICKs
        {
            // Take the last CLICK event
            let channel_leader_events = [CAMPAIGN_2_EVENTS[7].clone()];

            let channel_leader_response = post_new_events(
                &follower_sentry,
                token_chain_1337.clone().with(CAMPAIGN_2.id),
                // the Leader of this channel is FOLLOWER!
                &channel_leader_events,
            )
            .await
            .expect("Posted events");

            assert_eq!(SuccessResponse { success: true }, channel_leader_response);

            info!(
                setup.logger,
                "Successful POST of the last CLICK event for CAMPAIGN_2 {:?} and Channel {:?} to Leader to trigger Healthy NewState",
                CAMPAIGN_2.id,
                CAMPAIGN_2.channel.id()
            );
        }

        // IMPORTANT! Call the FOLLOWER tick first as it's the Channel 2 Leader!
        // This will trigger a the new state and the Channel 2 follower (LEADER) will process that.

        // follower single worker tick
        follower_worker.all_channels_tick().await;
        // leader single worker tick
        leader_worker.all_channels_tick().await;

        // For CAMPAIGN_2 Channel 2
        //
        // Healthy ApproveState
        //
        {
            let latest_approve_state_follower = leader_sentry
                .get_our_latest_msg(CAMPAIGN_2.channel.id(), &["ApproveState"])
                .await
                .expect(
                    "Should fetch ApproveState from Channel's Follower (Who am I) from LEADER sentry",
                )
                .map(|message| {
                    ApproveState::try_from(message)
                        .expect("Should be ApproveState with valid Checked Balances")
                })
                .expect("Should have a ApproveState in Channel's Follower for the Campaign 2 channel");

            assert!(
                latest_approve_state_follower.is_healthy,
                "ApproveState in Channel's Follower (LEADER) should be healthy"
            );

            let latest_new_state_leader = follower_sentry
                .get_our_latest_msg(CAMPAIGN_2.channel.id(), &["NewState"])
                .await
                .expect(
                    "Should fetch NewState from Channel's Leader (Who am I) from FOLLOWER sentry",
                )
                .map(|message| {
                    NewState::<CheckedState>::try_from(message)
                        .expect("Should be NewState with valid Checked Balances")
                })
                .expect("Should have a NewState in Channel's Follower for the Campaign 2 channel");

            assert_eq!(
                latest_new_state_leader.state_root,
                latest_approve_state_follower.state_root
            );

            // double check that the ApproveStateResponse in both validators is present
            // and that they are the same
            let last_approve_state_leader = follower_sentry
            .get_last_approved(CAMPAIGN_2.channel.id())
            .await
            .expect(
                "Should fetch Last Approved Response from Channel's Leader (Who am I) from FOLLOWER sentry",
            ).last_approved.expect("Should have an ApproveState & NewState");

            let last_approve_state_follower = leader_sentry
            .get_last_approved(CAMPAIGN_2.channel.id())
            .await
            .expect(
                "Should fetch Last Approved Response from Channel's Follower (Who am I) from LEADER sentry",
            ).last_approved.expect("Should have an ApproveState & NewState");

            // compare NewState from Leader & Follower
            // NOTE: The `received` timestamp can differ, so compare everything except `received`!
            {
                let leader = last_approve_state_leader.new_state.unwrap();
                let follower = last_approve_state_follower.new_state.unwrap();

                assert_eq!(
                    leader.msg, follower.msg,
                    "The NewState Messages in Channel's Leader & Follower should be the same"
                );
                assert_eq!(
                    leader.from,
                    follower.from,
                    "The NewState Messages in Channel's Leader & Follower should have the same `from` address"
                );
            }

            // compare ApproveState from Leader & Follower
            // NOTE: The `received` timestamp can differ, so compare everything except `received`!
            {
                let leader = last_approve_state_leader.approve_state.unwrap();
                let follower = last_approve_state_follower.approve_state.unwrap();

                assert_eq!(
                    leader.msg, follower.msg,
                    "The ApproveState Messages in Channel's Leader & Follower should be the same"
                );
                assert_eq!(
                    leader.from,
                    follower.from,
                    "The ApproveState Messages in Channel's Leader & Follower should have the same `from` address"
                );
            }
        }
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
        platform::PlatformApi,
        Application,
    };
    use slog::info;
    use subprocess::{Popen, PopenConfig, Redirection};

    use crate::TestValidator;

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

        let logger = new_logger(&validator.sentry_logger_prefix);
        let campaign_remaining = CampaignRemaining::new(redis.clone());

        let platform_api = PlatformApi::new(
            validator.config.platform.url.clone(),
            validator.config.platform.keep_alive_interval,
        )
        .expect("Failed to build PlatformApi");

        let app = Application::new(
            adapter,
            validator.config.clone(),
            logger,
            redis.clone(),
            postgres.clone(),
            campaign_remaining,
            platform_api,
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
