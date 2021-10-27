use std::net::{IpAddr, Ipv4Addr};

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
use primitives::{adapter::KeystoreOptions, config::TokenInfo, Address, Config};
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
    /// Prefix for the loggers
    pub logger_prefix: String,
    /// Postgres DB name
    /// The rest of the Postgres values are taken from env. variables
    pub db_name: String,
}

pub static VALIDATORS: Lazy<[TestValidator; 2]> = Lazy::new(|| {
    use adapter::ethereum::test_util::GANACHE_KEYSTORES;
    use primitives::config::Environment;

    [
        TestValidator {
            address: GANACHE_KEYSTORES["leader"].0,
            keystore: GANACHE_KEYSTORES["leader"].1.clone(),
            sentry_config: sentry::application::Config {
                env: Environment::Development,
                port: 8005,
                ip_addr: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                redis_url: "redis://127.0.0.1:6379/1".parse().unwrap(),
            },
            logger_prefix: "sentry-leader".into(),
            db_name: "sentry_leader".into(),
        },
        TestValidator {
            address: GANACHE_KEYSTORES["follower"].0,
            keystore: GANACHE_KEYSTORES["follower"].1.clone(),
            sentry_config: sentry::application::Config {
                env: Environment::Development,
                port: 8006,
                ip_addr: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
                redis_url: "redis://127.0.0.1:6379/2".parse().unwrap(),
            },
            logger_prefix: "sentry-follower".into(),
            db_name: "sentry_follower".into(),
        },
    ]
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
        test_util::{GANACHE_ADDRESSES, GANACHE_URL},
        EthereumAdapter,
    };
    use primitives::{
        adapter::Adapter,
        sentry::campaign_create::CreateCampaign,
        util::{tests::prep_db::DUMMY_VALIDATOR_LEADER, ApiUrl},
        BigNum, Channel, ChannelId,
    };
    use reqwest::{Client, StatusCode};

    #[tokio::test]
    #[ignore = "We use a snapshot, however, we have left this test for convenience"]
    async fn deploy_contracts() {
        let web3 = Web3::new(Http::new(&GANACHE_URL).expect("failed to init transport"));
        let setup = Setup { web3 };
        // deploy contracts
        let _contracts = setup.deploy_contracts().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn run_full_test() {
        let web3 = Web3::new(Http::new(&GANACHE_URL).expect("failed to init transport"));
        let setup = Setup { web3 };
        // Use snapshot contracts
        let contracts = SNAPSHOT_CONTRACTS.clone();

        let leader = VALIDATORS[0].clone();
        let follower = VALIDATORS[1].clone();

        let channel_1 = Channel {
            leader: leader.address.into(),
            follower: follower.address.into(),
            guardian: GANACHE_ADDRESSES["guardian"].into(),
            token: contracts.token.1,
            nonce: 0_u64.into(),
        };

        // switch the roles of the 2 validators & use a new guardian
        let channel_2 = Channel {
            leader: follower.address.into(),
            follower: leader.address.into(),
            guardian: GANACHE_ADDRESSES["guardian2"].into(),
            token: contracts.token.1,
            nonce: 1_u64.into(),
        };

        // setup deposits
        let token_precision = contracts.token.0.precision.get();

        // setup Sentry & return Adapter
        let leader_adapter = setup_sentry(leader).await;
        let _follower_adapter = setup_sentry(follower).await;

        // setup relayer
        // Relayer is used when getting a session from token in Adapter
        // TODO: impl

        // Creator deposit
        {
            // OUTPACE deposit = 10 * 10^18 = 10 TOKENs
            // Counterfactual deposit = 5 TOKENs
            let creator_deposit = Deposit {
                channel: channel_1,
                token: contracts.token.0.clone(),
                address: GANACHE_ADDRESSES["creator"],
                outpace_amount: BigNum::with_precision(10, token_precision),
                counterfactual_amount: BigNum::with_precision(5, token_precision),
            };
            setup.deposit(&contracts, &creator_deposit).await;

            // make sure we have the expected deposit returned from EthereumAdapter
            let creator_eth_deposit = leader_adapter
                .get_deposit(&channel_1, &creator_deposit.address)
                .await
                .expect("Should get deposit for creator");

            assert_eq!(creator_deposit, creator_eth_deposit);
        }

        // Advertiser deposits
        // Channel 1
        // Outpace: 20 TOKENs
        // Counterfactual: 10 TOKENs
        // Channel 2
        // Outpace: 30 TOKENs
        // Counterfactual: 40 TOKENs
        {
            let advertiser_deposits = [
                Deposit {
                    channel: channel_1,
                    token: contracts.token.0.clone(),
                    address: GANACHE_ADDRESSES["advertiser"],
                    outpace_amount: BigNum::with_precision(20, token_precision),
                    counterfactual_amount: BigNum::with_precision(10, token_precision),
                },
                Deposit {
                    channel: channel_2,
                    token: contracts.token.0.clone(),
                    address: GANACHE_ADDRESSES["advertiser"],
                    outpace_amount: BigNum::with_precision(30, token_precision),
                    counterfactual_amount: BigNum::with_precision(20, token_precision),
                },
            ];
            // 1st deposit
            {
                setup.deposit(&contracts, &advertiser_deposits[0]).await;

                // make sure we have the expected deposit returned from EthereumAdapter
                let eth_deposit = leader_adapter
                    .get_deposit(&channel_1, &advertiser_deposits[0].address)
                    .await
                    .expect("Should get deposit for advertiser");

                assert_eq!(advertiser_deposits[0], eth_deposit);
            }

            // 2nd deposit
            {
                setup.deposit(&contracts, &advertiser_deposits[1]).await;

                // make sure we have the expected deposit returned from EthereumAdapter
                let eth_deposit = leader_adapter
                    .get_deposit(&channel_2, &advertiser_deposits[1].address)
                    .await
                    .expect("Should get deposit for advertiser");

                assert_eq!(advertiser_deposits[1], eth_deposit);
            }
        }
        // Use `adapter.get_auth` for authentication!

        let api_client = reqwest::Client::new();
        let leader_url = DUMMY_VALIDATOR_LEADER
            .url
            .parse::<ApiUrl>()
            .expect("Valid url");

        // TODO: We should use a third adapter, e.g. "guardian", Instead of getting auth from leader for leader.
        let token = leader_adapter
            .get_auth(&leader_adapter.whoami())
            .expect("Get authentication");
        // No Channel 1 - 404
        // /v5/channel/{}/spender/all
        {
            let response = get_spender_all(&api_client, &leader_url, &token, channel_1.id())
                .await
                .expect("Should return Response");

            assert_eq!(StatusCode::NOT_FOUND, response.status());
        }

        // Creator 1 - Campaign 1 - Channel 1 only
        {
            // create Campaign - 400 - not enough budget
            // TODO: Try to create campaign

            // create Campaign - 200

            // create 2nd Campaign
        }

        // Channel 1 exists
        // TODO: call `spender/all` - 200

        // Creator - Channel 1 & Channel 2
        {
            // create Campaign for Channel 1 - 200

            // create Campaign for Channel 2 - 200
        }

        // setup sentry
        // let sentry_leader = run_sentry();

        // setup worker

        // run sentry
        // run worker single-tick
        // sentry_leader.kill().expect("Killed Sentry");
    }

    async fn setup_sentry(validator: TestValidator) -> EthereumAdapter {
        let mut adapter = EthereumAdapter::init(validator.keystore, &GANACHE_CONFIG)
            .expect("EthereumAdapter::init");

        adapter.unlock().expect("Unlock successfully adapter");

        run_sentry_app(
            adapter.clone(),
            &validator.logger_prefix,
            validator.sentry_config,
            &validator.db_name,
        )
        .await
        .expect("To run Sentry API server");

        adapter
    }

    async fn get_spender_all(
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
            .bearer_auth(&token)
            .send()
            .await?)
    }
}
pub mod run {
    use std::{env::current_dir, net::SocketAddr, path::PathBuf};

    use adapter::EthereumAdapter;
    use primitives::{ToETHChecksum, ValidatorId};
    use sentry::{
        application::{logger, run},
        db::{
            postgres_connection, redis_connection, tests_postgres::setup_test_migrations,
            CampaignRemaining, POSTGRES_HOST, POSTGRES_PASSWORD, POSTGRES_PORT, POSTGRES_USER,
        },
        Application,
    };
    use slog::info;
    use subprocess::{Popen, PopenConfig, Redirection};

    use crate::GANACHE_CONFIG;

    pub async fn run_sentry_app(
        adapter: EthereumAdapter,
        logger_prefix: &str,
        app_config: sentry::application::Config,
        db_name: &str,
    ) -> anyhow::Result<()> {
        let socket_addr = SocketAddr::new(app_config.ip_addr, app_config.port);

        let postgres_config = {
            let mut config = sentry::db::PostgresConfig::new();

            config
                .user(POSTGRES_USER.as_str())
                .password(POSTGRES_PASSWORD.as_str())
                .host(POSTGRES_HOST.as_str())
                .port(*POSTGRES_PORT)
                .dbname(db_name);

            config
        };

        let postgres = postgres_connection(42, postgres_config).await;
        let redis = redis_connection(app_config.redis_url).await?;
        let campaign_remaining = CampaignRemaining::new(redis.clone());

        setup_test_migrations(postgres.clone())
            .await
            .expect("Should run migrations");

        let app = Application::new(
            adapter,
            GANACHE_CONFIG.clone(),
            logger(logger_prefix),
            redis,
            postgres,
            campaign_remaining,
        );

        info!(&app.logger, "Spawn sentry Hyper server");
        tokio::spawn(run(app, socket_addr));

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
