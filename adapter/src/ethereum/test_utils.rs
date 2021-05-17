use lazy_static::lazy_static;
use std::{collections::HashMap, num::NonZeroU8};
use web3::{
    contract::{Contract, Options},
    ethabi::Token,
    transports::Http,
    types::{H160, H256, U256},
    Web3,
};

use primitives::{
    adapter::KeystoreOptions,
    channel_v5::{Channel, Nonce},
    config::{configuration, TokenInfo},
    Address, BigNum, ValidatorId,
};

use crate::EthereumAdapter;

use super::{EthereumChannel, OUTPACE_ABI, SWEEPER_ABI};

// See `adex-eth-protocol` `contracts/mocks/Token.sol`
lazy_static! {
    /// Mocked Token ABI
    pub static ref MOCK_TOKEN_ABI: &'static [u8] =
        include_bytes!("../../test/resources/mock_token_abi.json");
    /// Mocked Token bytecode in JSON
    pub static ref MOCK_TOKEN_BYTECODE: &'static str =
        include_str!("../../test/resources/mock_token_bytecode.bin");
    /// Sweeper bytecode
    pub static ref SWEEPER_BYTECODE: &'static str = include_str!("../../../lib/protocol-eth/resources/bytecode/Sweeper.bin");
    /// Outpace bytecode
    pub static ref OUTPACE_BYTECODE: &'static str = include_str!("../../../lib/protocol-eth/resources/bytecode/OUTPACE.bin");
    pub static ref GANACHE_ADDRESSES: HashMap<String, Address> = {
        vec![
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
        ]
        .into_iter()
        .collect()
    };
}

pub const GANACHE_URL: &'static str = "http://localhost:8545";

pub fn get_test_channel(token_address: Address) -> Channel {
    Channel {
        leader: ValidatorId::from(&GANACHE_ADDRESSES["leader"]),
        follower: ValidatorId::from(&GANACHE_ADDRESSES["follower"]),
        guardian: GANACHE_ADDRESSES["advertiser"],
        token: token_address,
        nonce: Nonce::from(12345_u32),
    }
}

pub fn setup_eth_adapter(
    sweeper_address: Option<[u8; 20]>,
    outpace_address: Option<[u8; 20]>,
    token_whitelist: Option<(Address, TokenInfo)>,
) -> EthereumAdapter {
    let mut config = configuration("development", None).expect("failed parse config");
    let keystore_options = KeystoreOptions {
        keystore_file: "./test/resources/keystore.json".to_string(),
        keystore_pwd: "adexvalidator".to_string(),
    };

    if let Some(address) = sweeper_address {
        config.sweeper_address = address;
    }

    if let Some(address) = outpace_address {
        config.outpace_address = address;
    }

    if let Some((address, token_info)) = token_whitelist {
        assert!(
            config
                .token_address_whitelist
                .insert(address, token_info)
                .is_none(),
            "It should not contain the generated token prior to this call!"
        )
    }

    EthereumAdapter::init(keystore_options, &config).expect("should init ethereum adapter")
}

pub async fn mock_set_balance(
    token_contract: &Contract<Http>,
    from: [u8; 20],
    address: [u8; 20],
    amount: u64,
) -> web3::contract::Result<H256> {
    tokio_compat_02::FutureExt::compat(token_contract.call(
        "setBalanceTo",
        (H160(address), U256::from(amount)),
        H160(from),
        Options::default(),
    ))
    .await
}

pub async fn outpace_deposit(
    outpace_contract: &Contract<Http>,
    channel: &Channel,
    to: [u8; 20],
    amount: u64,
) -> web3::contract::Result<H256> {
    tokio_compat_02::FutureExt::compat(outpace_contract.call(
        "deposit",
        (channel.tokenize(), H160(to), U256::from(amount)),
        H160(to),
        Options::with(|opt| {
            opt.gas_price = Some(1.into());
            // TODO: Check how much should this gas limit be!
            opt.gas = Some(61_721_975.into());
        }),
    ))
    .await
}

pub async fn sweeper_sweep(
    sweeper_contract: &Contract<Http>,
    outpace_address: [u8; 20],
    channel: &Channel,
    depositor: [u8; 20],
) -> web3::contract::Result<H256> {
    let from_leader_account = H160(*GANACHE_ADDRESSES["leader"].as_bytes());

    tokio_compat_02::FutureExt::compat(sweeper_contract.call(
        "sweep",
        (
            Token::Address(H160(outpace_address)),
            channel.tokenize(),
            Token::Array(vec![Token::Address(H160(depositor))]),
        ),
        from_leader_account,
        Options::with(|opt| {
            opt.gas_price = Some(1.into());
            // TODO: Check how much should this gas limit be!
            opt.gas = Some(6_721_975.into());
        }),
    ))
    .await
}

/// Deploys the Sweeper contract from `GANACHE_ADDRESS['leader']`
pub async fn deploy_sweeper_contract(
    web3: &Web3<Http>,
) -> web3::contract::Result<(H160, Contract<Http>)> {
    let from_leader_account = H160(*GANACHE_ADDRESSES["leader"].as_bytes());

    let feature = tokio_compat_02::FutureExt::compat(async {
        Contract::deploy(web3.eth(), &SWEEPER_ABI)
            .expect("Invalid ABI of Sweeper contract")
            .confirmations(0)
            .options(Options::with(|opt| {
                opt.gas_price = Some(1.into());
                opt.gas = Some(6_721_975.into());
            }))
            .execute(*SWEEPER_BYTECODE, (), from_leader_account)
    })
    .await;

    let sweeper_contract = tokio_compat_02::FutureExt::compat(feature).await?;

    Ok((sweeper_contract.address(), sweeper_contract))
}

/// Deploys the Outpace contract from `GANACHE_ADDRESS['leader']`
pub async fn deploy_outpace_contract(
    web3: &Web3<Http>,
) -> web3::contract::Result<(H160, Contract<Http>)> {
    let from_leader_account = H160(*GANACHE_ADDRESSES["leader"].as_bytes());

    let feature = tokio_compat_02::FutureExt::compat(async {
        Contract::deploy(web3.eth(), &OUTPACE_ABI)
            .expect("Invalid ABI of Sweeper contract")
            .confirmations(0)
            .options(Options::with(|opt| {
                opt.gas_price = Some(1.into());
                opt.gas = Some(6_721_975.into());
            }))
            .execute(*OUTPACE_BYTECODE, (), from_leader_account)
    })
    .await;

    let outpace_contract = tokio_compat_02::FutureExt::compat(feature).await?;

    Ok((outpace_contract.address(), outpace_contract))
}

/// Deploys the Mock Token contract from `GANACHE_ADDRESS['leader']`
pub async fn deploy_token_contract(
    web3: &Web3<Http>,
    min_token_units: u64,
) -> web3::contract::Result<(TokenInfo, H160, Contract<Http>)> {
    let from_leader_account = H160(*GANACHE_ADDRESSES["leader"].as_bytes());

    let feature = tokio_compat_02::FutureExt::compat(async {
        Contract::deploy(web3.eth(), &MOCK_TOKEN_ABI)
            .expect("Invalid ABI of Mock Token contract")
            .confirmations(0)
            .options(Options::with(|opt| {
                opt.gas_price = Some(1.into());
                opt.gas = Some(6_721_975.into());
            }))
            .execute(*MOCK_TOKEN_BYTECODE, (), from_leader_account)
    })
    .await;

    let token_contract = tokio_compat_02::FutureExt::compat(feature).await?;

    let token_info = TokenInfo {
        min_token_units_for_deposit: BigNum::from(min_token_units),
        precision: NonZeroU8::new(18).expect("should create NonZeroU8"),
    };

    Ok((token_info, token_contract.address(), token_contract))
}
