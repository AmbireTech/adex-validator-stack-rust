use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr},
};

use adapter::ethereum::{
    get_counterfactual_address,
    test_util::{
        deploy_outpace_contract, deploy_sweeper_contract, deploy_token_contract, mock_set_balance,
        outpace_deposit, GANACHE_URL, MOCK_TOKEN_ABI,
    },
    OUTPACE_ABI, SWEEPER_ABI,
};
use deposits::Deposit;
use once_cell::sync::Lazy;
use primitives::{adapter::KeystoreOptions, config::TokenInfo, util::ApiUrl, Address, Config};
use web3::{contract::Contract, transports::Http, types::H160, Web3};

pub mod deposits;

pub static GANACHE_CONFIG: Lazy<Config> = Lazy::new(|| {
    Config::try_toml(include_str!("../../docs/config/ganache.toml"))
        .expect("Failed to parse ganache.toml config file")
});

/// ganache-cli setup with deployed contracts using the snapshot directory
pub static SNAPSHOT_CONTRACTS: Lazy<Contracts> = Lazy::new(|| {
    use primitives::BigNum;
    use std::num::NonZeroU8;

    let web3 = Web3::new(Http::new(GANACHE_URL).expect("failed to init transport"));

    let token_address = "0x9db7bff788522dbe8fa2e8cbd568a58c471ccd5e"
        .parse::<Address>()
        .unwrap();
    let token = (
        // copied from deploy_token_contract
        TokenInfo {
            min_token_units_for_deposit: BigNum::from(10_u64.pow(18)),
            precision: NonZeroU8::new(18).expect("should create NonZeroU8"),
            // multiplier = 10^14 - 10^18 (token precision) = 10^-4
            // min_validator_fee = 1' * 10^-4 = 0.000_1
            min_validator_fee: BigNum::from(100_000_000_000_000),
        },
        token_address,
        Contract::from_json(web3.eth(), H160(token_address.to_bytes()), &MOCK_TOKEN_ABI).unwrap(),
    );

    let sweeper_address = "0xdd41b0069256a28972458199a3c9cf036384c156"
        .parse::<Address>()
        .unwrap();

    let sweeper = (
        sweeper_address,
        Contract::from_json(web3.eth(), H160(sweeper_address.to_bytes()), &SWEEPER_ABI).unwrap(),
    );

    let outpace_address = "0xcb097e455b7159f902e2eb45562fc397ae6b0f3d"
        .parse::<Address>()
        .unwrap();

    let outpace = (
        outpace_address,
        Contract::from_json(web3.eth(), H160(outpace_address.to_bytes()), &OUTPACE_ABI).unwrap(),
    );

    Contracts {
        token,
        sweeper,
        outpace,
    }
});

#[derive(Debug, Clone)]
pub struct TestValidator {
    pub address: Address,
    pub keystore: KeystoreOptions,
    pub sentry_config: sentry::application::Config,
    /// Sentry REST API url
    pub sentry_url: ApiUrl,
    /// Used for the _Sentry REST API_ [`sentry::Application`] as well as the _Validator worker_ [`validator_worker::worker::Args`]
    pub config: Config,
    /// Prefix for sentry logger
    pub sentry_logger_prefix: String,
    /// Prefix for validator worker logger
    pub worker_logger_prefix: String,
    /// Postgres DB name
    /// The rest of the Postgres values are taken from env. variables
    pub db_name: String,
}

pub static VALIDATORS: Lazy<HashMap<&'static str, TestValidator>> = Lazy::new(|| {
    use adapter::ethereum::test_util::GANACHE_KEYSTORES;
    use primitives::config::Environment;

    vec![
        (
            "leader",
            TestValidator {
                address: GANACHE_KEYSTORES["leader"].0,
                keystore: GANACHE_KEYSTORES["leader"].1.clone(),
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
            "follower",
            TestValidator {
                address: GANACHE_KEYSTORES["follower"].0,
                keystore: GANACHE_KEYSTORES["follower"].1.clone(),
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
    pub web3: Web3<Http>,
}

#[derive(Debug, Clone)]
pub struct Contracts {
    pub token: (TokenInfo, Address, Contract<Http>),
    pub sweeper: (Address, Contract<Http>),
    pub outpace: (Address, Contract<Http>),
}

impl Setup {
    pub async fn deploy_contracts(&self) -> Contracts {
        // deploy contracts
        // TOKEN contract is with precision 18 (like DAI)
        // set the minimum token units to 1 TOKEN
        let token = deploy_token_contract(&self.web3, 10_u64.pow(18))
            .await
            .expect("Correct parameters are passed to the Token constructor.");

        let sweeper = deploy_sweeper_contract(&self.web3)
            .await
            .expect("Correct parameters are passed to the Sweeper constructor.");

        let outpace = deploy_outpace_contract(&self.web3)
            .await
            .expect("Correct parameters are passed to the OUTPACE constructor.");

        Contracts {
            token,
            sweeper,
            outpace,
        }
    }

    pub async fn deposit(&self, contracts: &Contracts, deposit: &Deposit) {
        let counterfactual_address = get_counterfactual_address(
            contracts.sweeper.0,
            &deposit.channel,
            contracts.outpace.0,
            deposit.address,
        );

        // OUTPACE regular deposit
        // first set a balance of tokens to be deposited
        mock_set_balance(
            &contracts.token.2,
            deposit.address.to_bytes(),
            deposit.address.to_bytes(),
            &deposit.outpace_amount,
        )
        .await
        .expect("Failed to set balance");
        // call the OUTPACE deposit
        outpace_deposit(
            &contracts.outpace.1,
            &deposit.channel,
            deposit.address.to_bytes(),
            &deposit.outpace_amount,
        )
        .await
        .expect("Should deposit with OUTPACE");

        // Counterfactual address deposit
        mock_set_balance(
            &contracts.token.2,
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
    use adapter::ethereum::{
        test_util::{GANACHE_ADDRESSES, GANACHE_KEYSTORES, GANACHE_URL},
        EthereumAdapter,
    };
    use primitives::{
        adapter::Adapter,
        balances::CheckedState,
        sentry::{campaign_create::CreateCampaign, AccountingResponse},
        util::{logging::new_logger, ApiUrl},
        Balances, BigNum, Campaign, Channel, ChannelId, UnifiedNum,
    };
    use reqwest::{Client, StatusCode};
    use validator_worker::{sentry_interface::Validator, worker::Worker, SentryApi};

    #[tokio::test]
    #[ignore = "We use a snapshot, however, we have left this test for convenience"]
    async fn deploy_contracts() {
        let web3 = Web3::new(Http::new(GANACHE_URL).expect("failed to init transport"));
        let setup = Setup { web3 };
        // deploy contracts
        let _contracts = setup.deploy_contracts().await;
    }

    static CAMPAIGN_1: Lazy<Campaign> = Lazy::new(|| {
        use chrono::{TimeZone, Utc};
        use primitives::{
            campaign::{Active, Pricing, PricingBounds, Validators},
            targeting::Rules,
            validator::ValidatorDesc,
            EventSubmission,
        };

        let channel = Channel {
            leader: VALIDATORS["leader"].address.into(),
            follower: VALIDATORS["follower"].address.into(),
            guardian: GANACHE_ADDRESSES["guardian"],
            token: SNAPSHOT_CONTRACTS.token.1,
            nonce: 0_u64.into(),
        };

        let leader_desc = ValidatorDesc {
            id: VALIDATORS["leader"].address.into(),
            url: VALIDATORS["leader"].sentry_url.to_string(),
            // fee per 1000 (pro mille) = 0.03000000 (UnifiedNum)
            fee: 3_000_000.into(),
            fee_addr: None,
        };

        let follower_desc = ValidatorDesc {
            id: VALIDATORS["follower"].address.into(),
            url: VALIDATORS["follower"].sentry_url.to_string(),
            // fee per 1000 (pro mille) = 0.02000000 (UnifiedNum)
            fee: 2_000_000.into(),
            fee_addr: None,
        };

        let validators = Validators::new((leader_desc, follower_desc));

        Campaign {
            id: "0x936da01f9abd4d9d80c702af85c822a8"
                .parse()
                .expect("Should parse"),
            channel,
            creator: GANACHE_ADDRESSES["advertiser"],
            // 20.00000000
            budget: UnifiedNum::from(200_000_000),
            validators,
            title: Some("Dummy Campaign".to_string()),
            pricing_bounds: Some(PricingBounds {
                impression: Some(Pricing {
                    // 0.00000100
                    // Per 1000 = 0.00100000
                    min: 100.into(),
                    // 0.00000200
                    // Per 1000 = 0.00200000
                    max: 200.into(),
                }),
                click: Some(Pricing {
                    // 0.00000300
                    // Per 1000 = 0.00300000
                    min: 300.into(),
                    // 0.00000500
                    // Per 1000 = 0.00500000
                    max: 500.into(),
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

    /// This Campaign's Channel has switched leader & follower compared to [`CAMPAIGN_1`]
    ///
    /// `Channel.leader = VALIDATOR["follower"].address`
    /// `Channel.follower = VALIDATOR["leader"],address`
    /// See [`VALIDATORS`] for more details.
    static CAMPAIGN_2: Lazy<Campaign> = Lazy::new(|| {
        use chrono::{TimeZone, Utc};
        use primitives::{
            campaign::{Active, Pricing, PricingBounds, Validators},
            targeting::Rules,
            validator::ValidatorDesc,
            EventSubmission,
        };

        let channel = Channel {
            leader: VALIDATORS["follower"].address.into(),
            follower: VALIDATORS["leader"].address.into(),
            guardian: GANACHE_ADDRESSES["guardian2"],
            token: SNAPSHOT_CONTRACTS.token.1,
            nonce: 0_u64.into(),
        };

        // Uses the VALIDATORS["follower"] as the Leader for this Channel
        // switches the URL as well
        let leader_desc = ValidatorDesc {
            id: VALIDATORS["follower"].address.into(),
            url: VALIDATORS["follower"].sentry_url.to_string(),
            // fee per 1000 (pro mille) = 0.10000000 (UnifiedNum)
            fee: 10_000_000.into(),
            fee_addr: None,
        };

        // Uses the VALIDATORS["leader"] as the Follower for this Channel
        // switches the URL as well
        let follower_desc = ValidatorDesc {
            id: VALIDATORS["leader"].address.into(),
            url: VALIDATORS["leader"].sentry_url.to_string(),
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
            creator: GANACHE_ADDRESSES["advertiser"],
            // 20.00000000
            budget: UnifiedNum::from(2_000_000_000),
            validators,
            title: Some("Dummy Campaign".to_string()),
            pricing_bounds: Some(PricingBounds {
                impression: Some(Pricing {
                    // 0.00000100
                    // Per 1000 = 0.00100000
                    min: 100.into(),
                    // 0.00000200
                    // Per 1000 = 0.00200000
                    max: 200.into(),
                }),
                click: Some(Pricing {
                    // 0.00000300
                    // Per 1000 = 0.00300000
                    min: 300.into(),
                    // 0.00000500
                    // Per 1000 = 0.00500000
                    max: 500.into(),
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

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn run_full_test() {
        let web3 = Web3::new(Http::new(GANACHE_URL).expect("failed to init transport"));
        let setup = Setup { web3 };
        // Use snapshot contracts
        let contracts = SNAPSHOT_CONTRACTS.clone();

        let leader = VALIDATORS["leader"].clone();
        let follower = VALIDATORS["follower"].clone();

        let token_precision = contracts.token.0.precision.get();

        // We use the Advertiser's `EthereumAdapter::get_auth` for authentication!
        let mut advertiser_adapter =
            EthereumAdapter::init(GANACHE_KEYSTORES["advertiser"].1.clone(), &GANACHE_CONFIG)
                .expect("Should initialize creator adapter");
        advertiser_adapter
            .unlock()
            .expect("Should unlock advertiser's Ethereum Adapter");
        let advertiser_adapter = advertiser_adapter;

        // setup Sentry & returns Adapter
        let leader_adapter = setup_sentry(&leader).await;
        let follower_adapter = setup_sentry(&follower).await;

        // Advertiser deposits
        //
        // Channel 1:
        // - Outpace: 20 TOKENs
        // - Counterfactual: 10 TOKENs
        //
        // Channel 2:
        // - Outpace: 30 TOKENs
        // - Counterfactual: 20 TOKENs
        {
            let advertiser_deposits = [
                Deposit {
                    channel: CAMPAIGN_1.channel,
                    token: contracts.token.0.clone(),
                    address: advertiser_adapter.whoami().to_address(),
                    outpace_amount: BigNum::with_precision(20, token_precision),
                    counterfactual_amount: BigNum::with_precision(10, token_precision),
                },
                Deposit {
                    channel: CAMPAIGN_2.channel,
                    token: contracts.token.0.clone(),
                    address: advertiser_adapter.whoami().to_address(),
                    outpace_amount: BigNum::with_precision(30, token_precision),
                    counterfactual_amount: BigNum::with_precision(20, token_precision),
                },
            ];
            // 1st deposit
            {
                setup.deposit(&contracts, &advertiser_deposits[0]).await;

                // make sure we have the expected deposit returned from EthereumAdapter
                let eth_deposit = leader_adapter
                    .get_deposit(
                        &CAMPAIGN_1.channel,
                        &advertiser_adapter.whoami().to_address(),
                    )
                    .await
                    .expect("Should get deposit for advertiser");

                assert_eq!(advertiser_deposits[0], eth_deposit);
            }

            // 2nd deposit
            {
                setup.deposit(&contracts, &advertiser_deposits[1]).await;

                // make sure we have the expected deposit returned from EthereumAdapter
                let eth_deposit = leader_adapter
                    .get_deposit(
                        &CAMPAIGN_2.channel,
                        &advertiser_adapter.whoami().to_address(),
                    )
                    .await
                    .expect("Should get deposit for advertiser");

                assert_eq!(advertiser_deposits[1], eth_deposit);
            }
        }

        let api_client = reqwest::Client::new();

        // check Campaign Leader & Follower urls
        // they should be the same as the test validators
        // TODO: Should we use leader.sentry_url in the test directly after this check?
        let (leader_url, follower_url) = {
            let leader_url = CAMPAIGN_1.validators[0].try_api_url().expect("Valid url");
            let follower_url = CAMPAIGN_1.validators[1].try_api_url().expect("Valid url");

            assert_eq!(&leader.sentry_url, &leader_url);
            assert_eq!(&follower.sentry_url, &follower_url);

            (leader_url, follower_url)
        };

        // No Channel 1 - 404
        // GET /v5/channel/{}/spender/all
        {
            let leader_auth = advertiser_adapter
                .get_auth(&leader_adapter.whoami())
                .expect("Get authentication");

            let leader_response = get_spender_all_page_0(
                &api_client,
                &leader_url,
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
                .get_auth(&leader_adapter.whoami())
                .expect("Get authentication");

            let mut no_budget_campaign = CreateCampaign::from_campaign(CAMPAIGN_1.clone());
            // Deposit of Advertiser for Channel 2: 20 (outpace) + 10 (create2)
            // Campaign Budget: 40 TOKENs
            no_budget_campaign.budget = UnifiedNum::from(4_000_000_000);

            let no_budget_response =
                create_campaign(&api_client, &leader_url, &leader_auth, &no_budget_campaign)
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
            let leader_auth = advertiser_adapter
                .get_auth(&leader_adapter.whoami())
                .expect("Get authentication");

            let leader_response = get_spender_all_page_0(
                &api_client,
                &leader_url,
                &leader_auth,
                CAMPAIGN_1.channel.id(),
            )
            .await
            .expect("Should return Response");

            assert_eq!(StatusCode::OK, leader_response.status());
        }

        // Create Campaign 1 w/ Channel 1 using Advertiser
        // In Leader & Follower sentries
        // Response: 200 Ok
        {
            let create_campaign_1 = CreateCampaign::from_campaign(CAMPAIGN_1.clone());
            {
                let leader_token = advertiser_adapter
                    .get_auth(&leader_adapter.whoami())
                    .expect("Get authentication");

                let leader_response =
                    create_campaign(&api_client, &leader_url, &leader_token, &create_campaign_1)
                        .await
                        .expect("Should return Response");

                assert_eq!(StatusCode::OK, leader_response.status());
            }

            {
                let follower_token = advertiser_adapter
                    .get_auth(&follower_adapter.whoami())
                    .expect("Get authentication");

                let follower_response = create_campaign(
                    &api_client,
                    &follower_url,
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
                    .get_auth(&leader_adapter.whoami())
                    .expect("Get authentication");

                let leader_response =
                    create_campaign(&api_client, &leader_url, &leader_token, &create_campaign_2)
                        .await
                        .expect("Should return Response");
                let status = leader_response.status();

                assert_eq!(StatusCode::OK, status);
            }

            {
                let follower_token = advertiser_adapter
                    .get_auth(&follower_adapter.whoami())
                    .expect("Get authentication");

                let follower_response = create_campaign(
                    &api_client,
                    &follower_url,
                    &follower_token,
                    &create_campaign_2,
                )
                .await
                .expect("Should return Response");

                assert_eq!(StatusCode::OK, follower_response.status());
            }
        }

        let leader_sentry = {
            // should get self Auth from Leader's EthereumAdapter
            let leader_auth = leader_adapter
                .get_auth(&leader_adapter.whoami())
                .expect("Get authentication");
            let whoami_validator = Validator {
                url: leader.sentry_url.clone(),
                token: leader_auth,
            };

            SentryApi::new(
                leader_adapter.clone(),
                new_logger(&leader.worker_logger_prefix),
                leader.config.clone(),
                whoami_validator,
            )
            .expect("Should create new SentryApi for the Leader Worker")
        };

        let follower_sentry = {
            // should get self Auth from Follower's EthereumAdapter
            let follower_auth = follower_adapter
                .get_auth(&follower_adapter.whoami())
                .expect("Get authentication");
            let whoami_validator = Validator {
                url: follower.sentry_url.clone(),
                token: follower_auth,
            };

            SentryApi::new(
                follower_adapter.clone(),
                new_logger(&follower.worker_logger_prefix),
                follower.config.clone(),
                whoami_validator,
            )
            .expect("Should create new SentryApi for the Leader Worker")
        };

        let leader_worker = Worker::from_sentry(leader_sentry.clone());
        let follower_worker = Worker::from_sentry(follower_sentry.clone());

        // leader single worker tick
        leader_worker.all_channels_tick().await;
        // follower single worker tick
        follower_worker.all_channels_tick().await;

        // Channel 1 expected Accounting
        {
            let expected_accounting = AccountingResponse {
                balances: Balances::<CheckedState>::new(),
            };
            let actual_accounting = leader_sentry
                .get_accounting(CAMPAIGN_1.channel.id())
                .await
                .expect("Should get Channel Accounting");

            assert_eq!(expected_accounting, actual_accounting);
        }
    }

    // async fn get_accounting(api_client: &Client, url: &ApiUrl, token: &str, channel: ChannelId) -> anyhow::Result<AccountingResponse<CheckedState>> {
    //     let endpoint_url = url
    //         .join(&format!("v5/channel/{}/accounting", channel))
    //         .expect("valid endpoint");

    //     Ok(api_client
    //         .get(endpoint_url)
    //         .bearer_auth(&token)
    //         .send()
    //         .await?
    //     .json()
    //     .await?)
    // }

    async fn setup_sentry(validator: &TestValidator) -> EthereumAdapter {
        let mut adapter = EthereumAdapter::init(validator.keystore.clone(), &GANACHE_CONFIG)
            .expect("EthereumAdapter::init");

        adapter.unlock().expect("Unlock successfully adapter");

        run_sentry_app(adapter.clone(), &validator)
            .await
            .expect("To run Sentry API server");

        adapter
    }

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

    use adapter::EthereumAdapter;
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
        adapter: EthereumAdapter,
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

        let postgres = postgres_connection(42, postgres_config).await;
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
