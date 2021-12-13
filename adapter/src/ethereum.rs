use ethstore::{ethkey::Password, SafeAccount};

use once_cell::sync::Lazy;
use serde_json::Value;
use web3::signing::keccak256;

use crate::{UnlockedState, LockedState};

pub use {
    client::{get_counterfactual_address, Ethereum, Options},
    error::Error,
};

pub type UnlockedAdapter = crate::Adapter<client::Ethereum<UnlockedWallet>, UnlockedState>;
pub type LockedAdapter = crate::Adapter<client::Ethereum<LockedWallet>, LockedState>;
pub type LockedClient = client::Ethereum<LockedWallet>;
pub type UnlockedClient = client::Ethereum<UnlockedWallet>;

mod channel;
mod client;
mod error;

/// Ethereum Web Token
/// See <https://github.com/ethereum/EIPs/issues/1341>
///
/// This module implements the Ethereum Web Token with 2 difference:
/// - The signature includes the Ethereum signature mode, see [`crate::ethereum::ewt::ETH_SIGN_SUFFIX`]
/// - The message being signed is not the `header.payload` directly,
///   but the `keccak256("header.payload")`.
pub mod ewt;

#[cfg(any(test, feature = "test-util"))]
pub mod test_util;

pub static OUTPACE_ABI: Lazy<&'static [u8]> =
    Lazy::new(|| include_bytes!("../../lib/protocol-eth/abi/OUTPACE.json"));
pub static ERC20_ABI: Lazy<&'static [u8]> = Lazy::new(|| {
    include_str!("../../lib/protocol-eth/abi/ERC20.json")
        .trim_end_matches('\n')
        .as_bytes()
});
pub static SWEEPER_ABI: Lazy<&'static [u8]> =
    Lazy::new(|| include_bytes!("../../lib/protocol-eth/abi/Sweeper.json"));
pub static IDENTITY_ABI: Lazy<&'static [u8]> =
    Lazy::new(|| include_bytes!("../../lib/protocol-eth/abi/Identity5.2.json"));

/// Ready to use init code (i.e. decoded) for calculating the create2 address
pub static DEPOSITOR_BYTECODE_DECODED: Lazy<Vec<u8>> = Lazy::new(|| {
    let bytecode = include_str!("../../lib/protocol-eth/resources/bytecode/Depositor.bin");
    hex::decode(bytecode).expect("Decoded properly")
});

/// Hashes the passed message with the format of `Signed Data Standard`
/// See https://eips.ethereum.org/EIPS/eip-191
fn to_ethereum_signed(message: &[u8]) -> [u8; 32] {
    let eth = "\x19Ethereum Signed Message:\n";
    let message_length = message.len();

    let mut bytes = format!("{}{}", eth, message_length).into_bytes();
    bytes.extend(message);

    keccak256(&bytes)
}

#[derive(Debug, Clone)]
pub struct UnlockedWallet {
    wallet: SafeAccount,
    password: Password,
}

#[derive(Debug, Clone)]
pub enum LockedWallet {
    KeyStore { keystore: Value, password: Password },
    PrivateKey(String),
}

pub trait WalletState: Send + Sync {}
impl WalletState for UnlockedWallet {}
impl WalletState for LockedWallet {}
