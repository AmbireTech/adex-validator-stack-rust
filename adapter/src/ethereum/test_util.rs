use once_cell::sync::Lazy;
use std::{collections::HashMap, env::current_dir, num::NonZeroU8};
use web3::{
    contract::{Contract, Options},
    ethabi::Token,
    transports::Http,
    types::{H160, H256, U256},
    Web3,
};

use primitives::{
    adapter::KeystoreOptions,
    channel::{Channel, Nonce},
    config::TokenInfo,
    test_util::{ADVERTISER, CREATOR, FOLLOWER, GUARDIAN, GUARDIAN_2, LEADER, PUBLISHER},
    Address, BigNum, Config, ValidatorId,
};

use crate::EthereumAdapter;

use super::{EthereumChannel, OUTPACE_ABI, SWEEPER_ABI};

// See `adex-eth-protocol` `contracts/mocks/Token.sol`
/// Mocked Token ABI
pub static MOCK_TOKEN_ABI: Lazy<&'static [u8]> =
    Lazy::new(|| include_bytes!("../../test/resources/mock_token_abi.json"));
/// Mocked Token bytecode in JSON
pub static MOCK_TOKEN_BYTECODE: Lazy<&'static str> =
    Lazy::new(|| include_str!("../../test/resources/mock_token_bytecode.bin"));
/// Sweeper bytecode
pub static SWEEPER_BYTECODE: Lazy<&'static str> =
    Lazy::new(|| include_str!("../../../lib/protocol-eth/resources/bytecode/Sweeper.bin"));
/// Outpace bytecode
pub static OUTPACE_BYTECODE: Lazy<&'static str> =
    Lazy::new(|| include_str!("../../../lib/protocol-eth/resources/bytecode/OUTPACE.bin"));

/// Uses local `keystore.json` file and it's address for testing and working with [`EthereumAdapter`]
pub static KEYSTORE_IDENTITY: Lazy<(Address, KeystoreOptions)> = Lazy::new(|| {
    // The address of the keystore file in `adapter/test/resources/keystore.json`
    let address = Address::try_from("0x2bDeAFAE53940669DaA6F519373f686c1f3d3393")
        .expect("failed to parse id");

    let full_path = current_dir().unwrap();
    // it always starts in `adapter` folder because of the crate scope
    // even when it's in the workspace
    let mut keystore_file = full_path.parent().unwrap().to_path_buf();
    keystore_file.push("adapter/test/resources/keystore.json");

    (address, keystore_options("keystore.json", "adexvalidator"))
});

pub static KEYSTORES: Lazy<HashMap<Address, KeystoreOptions>> = Lazy::new(|| {
    vec![
        (
            *LEADER,
            keystore_options(&format!("{}_keystore.json", *LEADER), "ganache0"),
        ),
        (
            *FOLLOWER,
            keystore_options(&format!("{}_keystore.json", *FOLLOWER), "ganache1"),
        ),
        (
            *GUARDIAN,
            keystore_options(&format!("{}_keystore.json", *GUARDIAN), "ganache2"),
        ),
        (
            *CREATOR,
            keystore_options(&format!("{}_keystore.json", *CREATOR), "ganache3"),
        ),
        (
            *ADVERTISER,
            keystore_options(&format!("{}_keystore.json", *ADVERTISER), "ganache4"),
        ),
        (
            *PUBLISHER,
            keystore_options(&format!("{}_keystore.json", *PUBLISHER), "ganache5"),
        ),
        (
            *GUARDIAN_2,
            keystore_options(&format!("{}_keystore.json", *GUARDIAN_2), "ganache6"),
        ),
    ]
    .into_iter()
    .collect()
});

/// Local `ganache` is running at:
pub const GANACHE_URL: &str = "http://localhost:8545";

/// This helper function generates the correct path to the keystore file from this file.
///
/// The `file_name` located at `adapter/test/resources`
/// The `password` for the keystore file
fn keystore_options(file_name: &str, password: &str) -> KeystoreOptions {
    let full_path = current_dir().unwrap();
    // it always starts in `adapter` folder because of the crate scope
    // even when it's in the workspace
    let mut keystore_file = full_path.parent().unwrap().to_path_buf();
    keystore_file.push(format!("adapter/test/resources/{}", file_name));

    KeystoreOptions {
        keystore_file: keystore_file.display().to_string(),
        keystore_pwd: password.to_string(),
    }
}

pub fn get_test_channel(token_address: Address) -> Channel {
    Channel {
        leader: ValidatorId::from(&LEADER),
        follower: ValidatorId::from(&FOLLOWER),
        guardian: *GUARDIAN,
        token: token_address,
        nonce: Nonce::from(12345_u32),
    }
}

pub fn setup_eth_adapter(config: Config) -> EthereumAdapter {
    EthereumAdapter::init(KEYSTORE_IDENTITY.1.clone(), &config)
        .expect("should init ethereum adapter")
}

pub async fn mock_set_balance(
    token_contract: &Contract<Http>,
    from: [u8; 20],
    address: [u8; 20],
    amount: &BigNum,
) -> web3::contract::Result<H256> {
    let amount = U256::from_dec_str(&amount.to_string()).expect("Should create U256");

    token_contract
        .call(
            "setBalanceTo",
            (H160(address), amount),
            H160(from),
            Options::default(),
        )
        .await
}

pub async fn outpace_deposit(
    outpace_contract: &Contract<Http>,
    channel: &Channel,
    to: [u8; 20],
    amount: &BigNum,
) -> web3::contract::Result<H256> {
    let amount = U256::from_dec_str(&amount.to_string()).expect("Should create U256");

    outpace_contract
        .call(
            "deposit",
            (channel.tokenize(), H160(to), amount),
            H160(to),
            Options::with(|opt| {
                opt.gas_price = Some(1.into());
                opt.gas = Some(6_721_975.into());
            }),
        )
        .await
}

pub async fn sweeper_sweep(
    sweeper_contract: &Contract<Http>,
    outpace_address: [u8; 20],
    channel: &Channel,
    depositor: [u8; 20],
) -> web3::contract::Result<H256> {
    let from_leader_account = H160(*LEADER.as_bytes());

    sweeper_contract
        .call(
            "sweep",
            (
                Token::Address(H160(outpace_address)),
                channel.tokenize(),
                Token::Array(vec![Token::Address(H160(depositor))]),
            ),
            from_leader_account,
            Options::with(|opt| {
                opt.gas_price = Some(1.into());
                opt.gas = Some(6_721_975.into());
            }),
        )
        .await
}

/// Deploys the Sweeper contract from `GANACHE_ADDRESS['leader']`
pub async fn deploy_sweeper_contract(
    web3: &Web3<Http>,
) -> web3::contract::Result<(Address, Contract<Http>)> {
    let sweeper_contract = Contract::deploy(web3.eth(), &SWEEPER_ABI)
        .expect("Invalid ABI of Sweeper contract")
        .confirmations(0)
        .options(Options::with(|opt| {
            opt.gas_price = Some(1.into());
            opt.gas = Some(6_721_975.into());
        }))
        .execute(
            *SWEEPER_BYTECODE,
            (),
            H160(LEADER.to_bytes()),
        )
        .await?;

    let sweeper_address = Address::from(sweeper_contract.address().to_fixed_bytes());

    Ok((sweeper_address, sweeper_contract))
}

/// Deploys the Outpace contract from `GANACHE_ADDRESS['leader']`
pub async fn deploy_outpace_contract(
    web3: &Web3<Http>,
) -> web3::contract::Result<(Address, Contract<Http>)> {
    let outpace_contract = Contract::deploy(web3.eth(), &OUTPACE_ABI)
        .expect("Invalid ABI of Outpace contract")
        .confirmations(0)
        .options(Options::with(|opt| {
            opt.gas_price = Some(1.into());
            opt.gas = Some(6_721_975.into());
        }))
        .execute(
            *OUTPACE_BYTECODE,
            (),
            H160(LEADER.to_bytes()),
        )
        .await?;
    let outpace_address = Address::from(outpace_contract.address().to_fixed_bytes());

    Ok((outpace_address, outpace_contract))
}

/// Deploys the Mock Token contract from `GANACHE_ADDRESS['leader']`
pub async fn deploy_token_contract(
    web3: &Web3<Http>,
    min_token_units: u64,
) -> web3::contract::Result<(TokenInfo, Address, Contract<Http>)> {
    let token_contract = Contract::deploy(web3.eth(), &MOCK_TOKEN_ABI)
        .expect("Invalid ABI of Mock Token contract")
        .confirmations(0)
        .options(Options::with(|opt| {
            opt.gas_price = Some(1.into());
            opt.gas = Some(6_721_975.into());
        }))
        .execute(
            *MOCK_TOKEN_BYTECODE,
            (),
            H160(LEADER.to_bytes()),
        )
        .await?;

    let token_info = TokenInfo {
        min_token_units_for_deposit: BigNum::from(min_token_units),
        precision: NonZeroU8::new(18).expect("should create NonZeroU8"),
        // 0.000_1
        min_validator_fee: BigNum::from(100_000_000_000_000),
    };

    let token_address = Address::from(token_contract.address().to_fixed_bytes());

    Ok((token_info, token_address, token_contract))
}
