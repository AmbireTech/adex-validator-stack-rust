use once_cell::sync::Lazy;
use std::{collections::HashMap, env::current_dir, num::NonZeroU8};
use web3::{
    contract::{Contract, Options as ContractOptions},
    ethabi::Token,
    transports::Http,
    types::{H160, H256, U256},
    Web3,
};

use primitives::{
    channel::{Channel, Nonce},
    config::{ChainInfo, TokenInfo, GANACHE_CONFIG},
    test_util::{ADVERTISER, CREATOR, FOLLOWER, GUARDIAN, GUARDIAN_2, LEADER, PUBLISHER},
    Address, BigNum, Chain, ValidatorId,
};

use super::{
    channel::EthereumChannel,
    client::{ChainTransport, Options},
    IDENTITY_ABI, OUTPACE_ABI, SWEEPER_ABI,
};

// See `adex-eth-protocol` `contracts/mocks/Token.sol`
/// Mocked Token ABI
pub static MOCK_TOKEN_ABI: Lazy<&'static [u8]> =
    Lazy::new(|| include_bytes!("../../tests/resources/mock_token_abi.json"));
/// Mocked Token bytecode in JSON
pub static MOCK_TOKEN_BYTECODE: Lazy<&'static str> =
    Lazy::new(|| include_str!("../../tests/resources/mock_token_bytecode.bin"));
/// Sweeper bytecode
pub static SWEEPER_BYTECODE: Lazy<&'static str> =
    Lazy::new(|| include_str!("../../../lib/protocol-eth/resources/bytecode/Sweeper.bin"));
/// Outpace bytecode
pub static OUTPACE_BYTECODE: Lazy<&'static str> =
    Lazy::new(|| include_str!("../../../lib/protocol-eth/resources/bytecode/OUTPACE.bin"));
/// Identity bytecode
pub static IDENTITY_BYTECODE: Lazy<&'static str> =
    Lazy::new(|| include_str!("../../../lib/protocol-eth/resources/bytecode/Identity5.2.bin"));

/// Uses local `keystore.json` file and it's address for testing and working with [`crate::Ethereum`]
pub static KEYSTORE_IDENTITY: Lazy<(Address, Options)> = Lazy::new(|| {
    // The address of the keystore file in `adapter/test/resources/keystore.json`
    let address = Address::try_from("0x2bDeAFAE53940669DaA6F519373f686c1f3d3393")
        .expect("failed to parse id");

    let full_path = current_dir().unwrap();
    // it always starts in `adapter` folder because of the crate scope
    // even when it's in the workspace
    let mut keystore_file = full_path.parent().unwrap().to_path_buf();
    keystore_file.push("adapter/tests/resources/keystore.json");

    (address, keystore_options("keystore.json", "adexvalidator"))
});

pub static KEYSTORES: Lazy<HashMap<Address, Options>> = Lazy::new(|| {
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

// /// [`Chain`] of the locally running `ganache-cli` chain with id #1
pub static GANACHE_1: Lazy<Chain> = Lazy::new(|| GANACHE_INFO_1.chain.clone());

/// [`ChainInfo`] of the locally running `ganache-cli` chain with id #1
pub static GANACHE_INFO_1: Lazy<ChainInfo> = Lazy::new(|| {
    GANACHE_CONFIG
        .chains
        .get("Ganache #1")
        .expect("Ganache Local chain 1 not found")
        .clone()
});

/// [`Chain`] of the locally running `ganache-cli` chain with id #1337
pub static GANACHE_1337: Lazy<Chain> = Lazy::new(|| GANACHE_INFO_1337.chain.clone());

/// [`ChainInfo`] of the locally running `ganache-cli` chain with id #1337
pub static GANACHE_INFO_1337: Lazy<ChainInfo> = Lazy::new(|| {
    GANACHE_CONFIG
        .chains
        .get("Ganache #1337")
        .expect("Ganache Local chain 1337 not found")
        .clone()
});

/// Initialized Ganache [`Web3`] instance using a Http transport for usage in tests.
/// It uses the [`GANACHE_1337`] to initialize the client.
pub static GANACHE_WEB3: Lazy<Web3<Http>> = Lazy::new(|| {
    GANACHE_1337
        .init_web3()
        .expect("Should init the Web3 client")
});

/// This helper function generates the correct path to the keystore file from this file.
///
/// The `file_name` located at `adapter/test/resources`
/// The `password` for the keystore file
fn keystore_options(file_name: &str, password: &str) -> Options {
    let full_path = current_dir().unwrap();
    // it always starts in `adapter` folder because of the crate scope
    // even when it's in the workspace
    let mut keystore_file = full_path.parent().unwrap().to_path_buf();
    keystore_file.push(format!("adapter/tests/resources/{}", file_name));

    Options {
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
            ContractOptions::default(),
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
            ContractOptions::with(|opt| {
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
            ContractOptions::with(|opt| {
                opt.gas_price = Some(1.into());
                opt.gas = Some(6_721_975.into());
            }),
        )
        .await
}

/// Deploys the Sweeper contract from [`LEADER`]
pub async fn deploy_sweeper_contract(
    web3: &Web3<Http>,
) -> web3::contract::Result<(Address, Contract<Http>)> {
    let sweeper_contract = Contract::deploy(web3.eth(), &SWEEPER_ABI)
        .expect("Invalid ABI of Sweeper contract")
        .confirmations(0)
        .options(ContractOptions::with(|opt| {
            opt.gas_price = Some(1.into());
            opt.gas = Some(6_721_975.into());
        }))
        .execute(*SWEEPER_BYTECODE, (), H160(LEADER.to_bytes()))
        .await?;

    let sweeper_address = Address::from(sweeper_contract.address().to_fixed_bytes());

    Ok((sweeper_address, sweeper_contract))
}

/// Deploys the Outpace contract from [`LEADER`]
pub async fn deploy_outpace_contract(
    web3: &Web3<Http>,
) -> web3::contract::Result<(Address, Contract<Http>)> {
    let outpace_contract = Contract::deploy(web3.eth(), &OUTPACE_ABI)
        .expect("Invalid ABI of Outpace contract")
        .confirmations(0)
        .options(ContractOptions::with(|opt| {
            opt.gas_price = Some(1.into());
            opt.gas = Some(6_721_975.into());
        }))
        .execute(*OUTPACE_BYTECODE, (), H160(LEADER.to_bytes()))
        .await?;
    let outpace_address = Address::from(outpace_contract.address().to_fixed_bytes());

    Ok((outpace_address, outpace_contract))
}

/// Deploys the Identity contract for the give `for_address`
/// Adds privileges by the constructor for the `add_privileges_to` addresses
pub async fn deploy_identity_contract(
    web3: &Web3<Http>,
    for_address: Address,
    add_privileges_to: &[Address],
) -> web3::contract::Result<(Address, Contract<Http>)> {
    let add_privileges_to: Vec<_> = add_privileges_to
        .iter()
        .map(|a| Token::Address(H160(a.to_bytes())))
        .collect();

    let identity_contract = Contract::deploy(web3.eth(), &IDENTITY_ABI)
        .expect("Invalid ABI of Identity contract")
        .confirmations(0)
        .options(ContractOptions::with(|opt| {
            opt.gas_price = Some(1.into());
            opt.gas = Some(6_721_975.into());
        }))
        .execute(
            *IDENTITY_BYTECODE,
            Token::Array(add_privileges_to),
            H160(for_address.to_bytes()),
        )
        .await?;

    let identity_address = Address::from(identity_contract.address().to_fixed_bytes());

    Ok((identity_address, identity_contract))
}

/// Deploys the Mock Token contract from [`LEADER`]
pub async fn deploy_token_contract(
    web3: &Web3<Http>,
    min_token_units: u64,
) -> web3::contract::Result<(TokenInfo, Address, Contract<Http>)> {
    let token_contract = Contract::deploy(web3.eth(), &MOCK_TOKEN_ABI)
        .expect("Invalid ABI of Mock Token contract")
        .confirmations(0)
        .options(ContractOptions::with(|opt| {
            opt.gas_price = Some(1_i32.into());
            opt.gas = Some(6_721_975_i32.into());
        }))
        .execute(*MOCK_TOKEN_BYTECODE, (), H160(LEADER.to_bytes()))
        .await?;

    let token_address = Address::from(token_contract.address().to_fixed_bytes());

    let token_info = TokenInfo {
        min_token_units_for_deposit: BigNum::from(min_token_units),
        precision: NonZeroU8::new(18).expect("should create NonZeroU8"),
        // 0.000_001
        min_validator_fee: BigNum::from(1_000_000_000_000),
        address: token_address,
    };

    Ok((token_info, token_address, token_contract))
}
