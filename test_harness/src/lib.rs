use adapter::ethereum::{
    test_util::{
        deploy_outpace_contract, deploy_sweeper_contract, deploy_token_contract, GANACHE_KEYSTORES,
        GANACHE_URL, MOCK_TOKEN_ABI,
    },
    EthereumAdapter, OUTPACE_ABI, SWEEPER_ABI,
};
use once_cell::sync::Lazy;
use primitives::{
    config::{TokenInfo, DEVELOPMENT_CONFIG},
    Address,
};
use web3::{contract::Contract, transports::Http, types::H160, Web3};

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
            // 0.000_1
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

        let mut eth_adapter = EthereumAdapter::init(GANACHE_KEYSTORES["leader"].1.clone(), &config)
            .expect("Should Sentry::init");

        // eth_adapter.unlock().expect("should unlock eth adapter");

        eth_adapter
    }
}

// pub async fn set_token_deposit(
//     token: Contract<Http>,
//     (from, counterfactual_address): (Address, Address),
//     amount: u64,
// ) {

// let deposit_with_create2 = eth_adapter
//     .get_deposit(&channel, &spender)
//     .await
//     .expect("should get deposit");

// assert_eq!(
//     Deposit {
//         total: BigNum::from(11_999),
//         // tokens are more than the minimum tokens required for deposits to count
//         still_on_create2: BigNum::from(1_999),
//     },
//     deposit_with_create2
// );
// }

#[cfg(test)]
mod tests {
    use super::*;
    use adapter::ethereum::{
        get_counterfactual_address,
        test_util::{mock_set_balance, outpace_deposit, GANACHE_ADDRESSES, GANACHE_URL},
    };
    use primitives::{adapter::Adapter, BigNum, Channel, Deposit};

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

        let channel = Channel {
            leader: GANACHE_ADDRESSES["leader"].into(),
            follower: GANACHE_ADDRESSES["follower"].into(),
            guardian: GANACHE_ADDRESSES["guardian"].into(),
            token: contracts.token.1,
            nonce: 0_u64.into(),
        };

        let precision_multiplier = 10_u64.pow(contracts.token.0.precision.get().into());
        // setup deposits
        // OUTPACE deposit = 10 * 10^18 = 10 TOKENS
        let (creator, creator_deposit) = (GANACHE_ADDRESSES["creator"], 10 * precision_multiplier);
        // Counterfactual deposit = 5 TOKENS
        let (counterfactual_address, counterfactual_deposit) = (
            get_counterfactual_address(contracts.sweeper.0, &channel, contracts.outpace.0, creator),
            5 * precision_multiplier,
        );
        // OUTPACE regular deposit
        {
            // first set a balance of tokens to be deposited
            mock_set_balance(
                &contracts.token.2,
                creator.to_bytes(),
                creator.to_bytes(),
                creator_deposit,
            )
            .await
            .expect("Failed to set balance");
            // call the OUTPACE deposit
            outpace_deposit(
                &contracts.outpace.1,
                &channel,
                creator.to_bytes(),
                creator_deposit,
            )
            .await
            .expect("Should deposit with OUTPACE");

            // Counterfactual address deposit
            mock_set_balance(
                &contracts.token.2,
                creator.to_bytes(),
                counterfactual_address.to_bytes(),
                counterfactual_deposit,
            )
            .await
            .expect("Failed to set balance");
        }

        // setup relayer
        // setup Adapter
        let adapter = setup.adapter(&contracts);

        // make sure we have the expected deposit returned from EthereumAdapter
        let creator_eth_deposit = adapter
            .get_deposit(&channel, &creator)
            .await
            .expect("Should get deposit for creator");

        assert_eq!(
            Deposit::<BigNum> {
                total: BigNum::from(creator_deposit + counterfactual_deposit),
                still_on_create2: BigNum::from(counterfactual_deposit),
            },
            creator_eth_deposit
        );

        // Use `adapter.get_auth` for authentication!

        // setup sentry
        // let sentry_leader = run_sentry();

        // setup worker

        // run sentry
        // run worker single-tick
    }
}

// mod run {
//     use primitives::adapter::KeystoreOptions;
//     use subprocess::{Popen, PopenConfig, Redirection};

//     fn run_sentry(keystore_options: &KeystoreOptions, config) {
//         let sentry_leader = Popen::create(
//             &[
//                 "POSTGRES_DB=sentry_leader",
//                 "PORT=8005",
//                 &format!("KEYSTORE_PWD={}", keystore_options.keystore_pwd),
//                 "cargo",
//                 "run",
//                 "-p",
//                 "sentry",
//                 "--",
//                 "--adapter",
//                 "ethereum",
//                 "--keystoreFile",
//                 &keystore_options.keystore_file,
//                 "./docs/config/dev.toml",
//             ],
//             PopenConfig {
//                 stdout: Redirection::Pipe,
//                 ..Default::default()
//             },
//         );
//     }
// }