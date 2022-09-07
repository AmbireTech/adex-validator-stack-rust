//! The [`Ethereum`] client for the [`Adapter`].

use ethsign::{KeyFile, Protected, SecretKey, Signature};

use once_cell::sync::Lazy;
use web3::signing::keccak256;

use crate::{Adapter, LockedState, UnlockedState};

pub use {
    client::{Ethereum, Options},
    error::Error,
};

pub type UnlockedAdapter = Adapter<client::Ethereum<UnlockedWallet>, UnlockedState>;
pub type LockedAdapter = Adapter<client::Ethereum<LockedWallet>, LockedState>;
pub type LockedClient = Ethereum<LockedWallet>;
pub type UnlockedClient = Ethereum<UnlockedWallet>;

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
#[cfg_attr(docsrs, doc(cfg(feature = "test-util")))]
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

/// Hashes the passed message with the format of `Signed Data Standard`
/// See https://eips.ethereum.org/EIPS/eip-191
fn to_ethereum_signed(message: &[u8]) -> [u8; 32] {
    let eth = "\x19Ethereum Signed Message:\n";
    let message_length = message.len();

    let mut bytes = format!("{}{}", eth, message_length).into_bytes();
    bytes.extend(message);

    keccak256(&bytes)
}

/// Trait for encoding a signature into RSV array
/// and V altered to be in "Electrum" notation, i.e. `v += 27`
pub trait Electrum {
    /// Encode the signature into RSV array (V altered to be in "Electrum" notation).
    ///`{r}{s}{v}`
    ///
    ///
    fn to_electrum(&self) -> [u8; 65];

    /// Parse bytes as a signature encoded as RSV (V in "Electrum" notation).
    /// `{r}{s}{v}`
    ///
    /// Will return `None` if given data has invalid length or
    /// if `V` (byte 64) component is not in electrum notation (`< 27`)
    fn from_electrum(data: &[u8]) -> Option<Self>
    where
        Self: Sized;
}

impl Electrum for Signature {
    fn to_electrum(&self) -> [u8; 65] {
        let mut electrum_array = [0_u8; 65];

        // R
        electrum_array[0..32].copy_from_slice(&self.r);
        // S
        electrum_array[32..64].copy_from_slice(&self.s);
        // V altered to be in "Electrum" notation
        electrum_array[64] = self.v + 27;

        electrum_array
    }

    fn from_electrum(sig: &[u8]) -> Option<Self> {
        if sig.len() != 65 || sig[64] < 27 {
            return None;
        }

        let mut r = [0u8; 32];
        r.copy_from_slice(&sig[0..32]);

        let mut s = [0u8; 32];
        s.copy_from_slice(&sig[32..64]);

        let v = sig[64] - 27;

        Some(Signature { v, r, s })
    }
}

#[derive(Debug, Clone)]
pub struct UnlockedWallet {
    wallet: SecretKey,
}

#[derive(Debug, Clone)]
pub enum LockedWallet {
    KeyStore {
        keystore: KeyFile,
        password: Protected,
    },
    PrivateKey(String),
}

pub trait WalletState: Send + Sync {}
impl WalletState for UnlockedWallet {}
impl WalletState for LockedWallet {}
