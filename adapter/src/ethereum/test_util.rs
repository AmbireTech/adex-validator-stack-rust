use once_cell::sync::Lazy;
use std::{collections::HashMap, convert::TryFrom, env::current_dir, num::NonZeroU8};
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

pub static GANACHE_KEYSTORES: Lazy<HashMap<String, (Address, KeystoreOptions)>> = Lazy::new(|| {
    vec![
        (
            "guardian".to_string(),
            (
                "0xDf08F82De32B8d460adbE8D72043E3a7e25A3B39"
                    .parse()
                    .expect("Valid Address"),
                keystore_options(
                    "0xDf08F82De32B8d460adbE8D72043E3a7e25A3B39_keystore.json",
                    "address0",
                ),
            ),
        ),
        (
            "leader".to_string(),
            (
                "0x5a04A8fB90242fB7E1db7d1F51e268A03b7f93A5"
                    .parse()
                    .expect("Valid Address"),
                keystore_options(
                    "0x5a04A8fB90242fB7E1db7d1F51e268A03b7f93A5_keystore.json",
                    "address1",
                ),
            ),
        ),
        (
            "follower".to_string(),
            (
                "0xe3896ebd3F32092AFC7D27e9ef7b67E26C49fB02"
                    .parse()
                    .expect("Valid Address"),
                keystore_options(
                    "0xe3896ebd3F32092AFC7D27e9ef7b67E26C49fB02_keystore.json",
                    "address2",
                ),
            ),
        ),
        (
            "creator".to_string(),
            (
                "0x0E45891a570Af9e5A962F181C219468A6C9EB4e1"
                    .parse()
                    .expect("Valid Address"),
                keystore_options(
                    "0x0E45891a570Af9e5A962F181C219468A6C9EB4e1_keystore.json",
                    "address3",
                ),
            ),
        ),
        (
            "advertiser".to_string(),
            (
                "0x8c4B95383a46D30F056aCe085D8f453fCF4Ed66d"
                    .parse()
                    .expect("Valid Address"),
                keystore_options(
                    "0x8c4B95383a46D30F056aCe085D8f453fCF4Ed66d_keystore.json",
                    "address4",
                ),
            ),
        ),
        (
            "guardian2".to_string(),
            (
                "0x1059B025E3F8b8f76A8120D6D6Fd9fBa172c80b8"
                    .parse()
                    .expect("Valid Address"),
                keystore_options(
                    "0x1059B025E3F8b8f76A8120D6D6Fd9fBa172c80b8_keystore.json",
                    "address5",
                ),
            ),
        ),
    ]
    .into_iter()
    .collect()
});

/// Addresses generated on local running `ganache` for testing purposes.
/// see the `ganache-cli.sh` script in the repository
pub static GANACHE_ADDRESSES: Lazy<HashMap<String, Address>> = Lazy::new(|| {
    vec![
        (
            "guardian".to_string(),
            "0xDf08F82De32B8d460adbE8D72043E3a7e25A3B39"
                .parse()
                .expect("Valid Address"),
        ),
        (
            "leader".to_string(),
            "0x5a04A8fB90242fB7E1db7d1F51e268A03b7f93A5"
                .parse()
                .expect("Valid Address"),
        ),
        (
            "follower".to_string(),
            "0xe3896ebd3F32092AFC7D27e9ef7b67E26C49fB02"
                .parse()
                .expect("Valid Address"),
        ),
        (
            "creator".to_string(),
            "0x0E45891a570Af9e5A962F181C219468A6C9EB4e1"
                .parse()
                .expect("Valid Address"),
        ),
        (
            "advertiser".to_string(),
            "0x8c4B95383a46D30F056aCe085D8f453fCF4Ed66d"
                .parse()
                .expect("Valid Address"),
        ),
        (
            "guardian2".to_string(),
            "0x1059B025E3F8b8f76A8120D6D6Fd9fBa172c80b8"
                .parse()
                .expect("Valid Address"),
        )
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
        leader: ValidatorId::from(&GANACHE_ADDRESSES["leader"]),
        follower: ValidatorId::from(&GANACHE_ADDRESSES["follower"]),
        guardian: GANACHE_ADDRESSES["advertiser"],
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
    let from_leader_account = H160(*GANACHE_ADDRESSES["leader"].as_bytes());

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
            H160(GANACHE_ADDRESSES["leader"].to_bytes()),
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
            H160(GANACHE_ADDRESSES["leader"].to_bytes()),
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
            H160(GANACHE_ADDRESSES["leader"].to_bytes()),
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
