use adapter::ethereum::{
    get_counterfactual_address,
    test_util::{
        deploy_outpace_contract, deploy_sweeper_contract, deploy_token_contract, mock_set_balance,
        outpace_deposit, GANACHE_KEYSTORES, GANACHE_URL, MOCK_TOKEN_ABI,
    },
    EthereumAdapter, OUTPACE_ABI, SWEEPER_ABI,
};
use deposits::Deposit;
use once_cell::sync::Lazy;
use primitives::{
    config::{TokenInfo, DEVELOPMENT_CONFIG},
    Address, Config,
};
use web3::{contract::Contract, transports::Http, types::H160, Web3};
// use validator_worker::{sentry_interface::SentryApi};

pub mod deposits;

pub static GANACHE_CONFIG: Lazy<Config> = Lazy::new(|| {
    Config::try_toml(include_str!("../../docs/config/ganache.toml"))
        .expect("Failed to parse ganache.toml config file")
});

/// ganache-cli setup with deployed contracts using the snapshot directory
pub static SNAPSHOT_CONTRACTS: Lazy<Contracts> = Lazy::new(|| {
    use primitives::BigNum;
    use std::num::NonZeroU8;

    let web3 = Web3::new(Http::new(&GANACHE_URL).expect("failed to init transport"));

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

    /// Initializes the Ethereum Adapter and `unlock()`s it ready to be used.
    #[deprecated = "We now use GANACHE config directly."]
    pub fn adapter(&self, contracts: &Contracts) -> EthereumAdapter {
        // Development uses a locally running ganache-cli
        let mut config = DEVELOPMENT_CONFIG.clone();
        config.sweeper_address = contracts.sweeper.0.to_bytes();
        config.outpace_address = contracts.outpace.0.to_bytes();

        assert!(
            config
                .token_address_whitelist
                .insert(contracts.token.1, contracts.token.0.clone())
                .is_none(),
            "The Address of the just deployed token should not be present in Config"
        );

        let eth_adapter = EthereumAdapter::init(GANACHE_KEYSTORES["leader"].1.clone(), &config)
            .expect("Should Sentry::init");

        // eth_adapter.unlock().expect("should unlock eth adapter");

        eth_adapter
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
    use crate::run::run_sentry;

    use super::*;
    use adapter::ethereum::test_util::{GANACHE_ADDRESSES, GANACHE_URL};
    use primitives::{
        adapter::Adapter,
        sentry::AllSpendersResponse,
        util::{tests::prep_db::DUMMY_VALIDATOR_LEADER, ApiUrl},
        BigNum, Channel,
    };
    use reqwest::StatusCode;

    #[tokio::test]
    async fn deploy_contracts() {
        let web3 = Web3::new(Http::new(&GANACHE_URL).expect("failed to init transport"));
        let setup = Setup { web3 };
        // deploy contracts
        let _contracts = setup.deploy_contracts().await;
    }

    #[tokio::test]
    async fn my_test() {
        let web3 = Web3::new(Http::new(&GANACHE_URL).expect("failed to init transport"));
        let setup = Setup { web3 };
        // Use snapshot contracts
        let contracts = SNAPSHOT_CONTRACTS.clone();

        let channel_1 = Channel {
            leader: GANACHE_ADDRESSES["leader"].into(),
            follower: GANACHE_ADDRESSES["follower"].into(),
            guardian: GANACHE_ADDRESSES["guardian"].into(),
            token: contracts.token.1,
            nonce: 0_u64.into(),
        };

        let channel_2 = Channel {
            leader: GANACHE_ADDRESSES["leader"].into(),
            follower: GANACHE_ADDRESSES["follower"].into(),
            guardian: GANACHE_ADDRESSES["guardian2"].into(),
            token: contracts.token.1,
            nonce: 1_u64.into(),
        };

        // setup deposits
        let token_precision = contracts.token.0.precision.get();

        // setup relayer
        // setup Adapter
        let adapter = EthereumAdapter::init(GANACHE_KEYSTORES["leader"].1.clone(), &GANACHE_CONFIG)
            .expect("Should Sentry::init");
        let mut sentry_leader =
            run_sentry(&GANACHE_KEYSTORES["leader"].1).expect("Should run Sentry Leader");

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
            let creator_eth_deposit = adapter
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
                let eth_deposit = adapter
                    .get_deposit(&channel_1, &advertiser_deposits[0].address)
                    .await
                    .expect("Should get deposit for advertiser");

                assert_eq!(advertiser_deposits[0], eth_deposit);
            }

            // 2nd deposit
            {
                setup.deposit(&contracts, &advertiser_deposits[1]).await;

                // make sure we have the expected deposit returned from EthereumAdapter
                let eth_deposit = adapter
                    .get_deposit(&channel_2, &advertiser_deposits[1].address)
                    .await
                    .expect("Should get deposit for advertiser");

                assert_eq!(advertiser_deposits[1], eth_deposit);
            }
        }
        // Use `adapter.get_auth` for authentication!

        // TODO: call `spender/all`

        let api_client = reqwest::Client::new();
        let leader_url = DUMMY_VALIDATOR_LEADER
            .url
            .parse::<ApiUrl>()
            .expect("Valid url");
        // No Channel 1 - 404
        // /v5/channel/{}/spender/all
        {
            let url = leader_url
                .join(&format!("v5/channel/{}/spender/all", channel_1.id()))
                .expect("valid endpoint");

            let response = api_client.get(url).send().await.expect("Valid response");

            assert_eq!(StatusCode::NOT_FOUND, response.status());
            //.json::<AllSpendersResponse>().await.expect("Valid JSON response");
        }

        // Creator 1 - Channel 1 only
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
        sentry_leader.kill().expect("Killed Sentry");
    }
}

pub mod run {
    use std::{env::current_dir, path::PathBuf};

    use primitives::adapter::KeystoreOptions;
    use subprocess::{Popen, PopenConfig, Redirection};

    /// This helper function generates the correct path to the keystore file from this file.
    ///
    /// The `file_name` located at `adapter/test/resources`
    fn keystore_file_path(file_name: &str) -> PathBuf {
        let full_path = current_dir().unwrap();
        // it always starts in `adapter` folder because of the crate scope
        // even when it's in the workspace
        let mut keystore_file = full_path.parent().unwrap().to_path_buf();
        keystore_file.push(format!("adapter/test/resources/{}", file_name));

        keystore_file
    }
    fn project_file_path(file: &str) -> PathBuf {
        let full_path = current_dir().unwrap();
        let project_path = full_path.parent().unwrap().to_path_buf();

        project_path.join(file)
    }


    // POSTGRES_DB=sentry_leader PORT=8005 KEYSTORE_PWD=address1 \
    // cargo run -p sentry -- --adapter ethereum --keystoreFile ./adapter/test/resources/5a04A8fB90242fB7E1db7d1F51e268A03b7f93A5_keystore.json \
    // ./docs/config/ganache.toml
    ///
    /// ```bash
    /// POSTGRES_DB=sentry_leader PORT=8005 KEYSTORE_PWD=address1 \
    /// cargo run -p sentry -- --adapter ethereum --keystoreFile ./adapter/test/resources/5a04A8fB90242fB7E1db7d1F51e268A03b7f93A5_keystore.json \
    /// ./docs/config/ganache.toml
    /// ```
    pub fn run_sentry(keystore_options: &KeystoreOptions) -> anyhow::Result<Popen> {
        let sentry_leader = Popen::create(
            &[
                "POSTGRES_DB=sentry_leader",
                "PORT=8005",
                &format!("KEYSTORE_PWD={}", keystore_options.keystore_pwd),
                "cargo",
                "run",
                "-p",
                "sentry",
                "--",
                "--adapter",
                "ethereum",
                "--keystoreFile",
                &dbg!(
                    keystore_file_path("5a04A8fB90242fB7E1db7d1F51e268A03b7f93A5_keystore.json")
                        .to_string_lossy()
                ),
                &dbg!(project_file_path("docs/config/ganache.toml").to_string_lossy()),
            ],
            PopenConfig {
                stdout: Redirection::Pipe,
                ..Default::default()
            },
        )?;

        Ok(sentry_leader)
    }
}
