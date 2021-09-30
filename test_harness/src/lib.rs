use adapter::ethereum::{
    test_util::{
        deploy_outpace_contract, deploy_sweeper_contract, deploy_token_contract, KEYSTORE_IDENTITY,
    },
    EthereumAdapter,
};
use primitives::{
    adapter::Adapter,
    config::{TokenInfo, DEVELOPMENT_CONFIG},
    Address,
};
use web3::{contract::Contract, transports::Http, types::H160, Web3};
pub struct Setup {
    pub web3: Web3<Http>,
}

pub struct Contracts {
    pub token: (TokenInfo, Address, Contract<Http>),
    pub sweeper: (H160, Contract<Http>),
    pub outpace: (H160, Contract<Http>),
}

impl Setup {
    pub async fn contracts(&self) -> Contracts {
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
        let mut config = DEVELOPMENT_CONFIG.clone();
        config.sweeper_address = contracts.sweeper.0.to_fixed_bytes();
        config.outpace_address = contracts.outpace.0.to_fixed_bytes();
        assert!(
            config
                .token_address_whitelist
                .insert(contracts.token.1, contracts.token.0.clone())
                .is_none(),
            "The Address of the just deployed token should not be present in Config"
        );

        // TODO: Ganache CLI
        let mut eth_adapter = EthereumAdapter::init(KEYSTORE_IDENTITY.1.clone(), &config)
            .expect("Should Sentry::init");
        eth_adapter.unlock().expect("should unlock eth adapter");

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
    use primitives::{util::tests::prep_db::DUMMY_CAMPAIGN, BigNum, Deposit};

    #[tokio::test]
    async fn my_test() {
        // TODO: Figure out how to create the Keystore address in Ganache from the keystore.json file
        let web3 = Web3::new(Http::new(&GANACHE_URL).expect("failed to init transport"));
        let setup = Setup { web3 };
        // setup contracts
        let contracts = setup.contracts().await;
        let mut channel = DUMMY_CAMPAIGN.channel;
        channel.token = contracts.token.1;

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
        }

        // Counterfactual address deposit
        mock_set_balance(
            &contracts.token.2,
            creator.to_bytes(),
            counterfactual_address.to_fixed_bytes(),
            counterfactual_deposit,
        )
        .await
        .expect("Failed to set balance");

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

        // setup worker

        // run sentry
        // run worker single-tick

        //
    }
}
