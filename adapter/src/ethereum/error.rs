use crate::Error as AdapterError;
use primitives::{
    address::Error as AddressError, big_num::ParseBigIntError, ChainId, ChannelId, ValidatorId,
};
use thiserror::Error;

use super::ewt::Payload;

impl From<Error> for AdapterError {
    fn from(error: Error) -> Self {
        match error {
            err @ Error::Keystore(..) => AdapterError::adapter(err),
            Error::WalletUnlock(err) => AdapterError::wallet_unlock(err),
            err @ Error::Web3(..) => AdapterError::adapter(err),
            err @ Error::InvalidChannelId { .. } => AdapterError::adapter(err),
            err @ Error::ChannelInactive(..) => AdapterError::adapter(err),
            err @ Error::ChainNotWhitelisted(..) => AdapterError::adapter(err),
            err @ Error::InvalidDepositAsset(..) => AdapterError::adapter(err),
            err @ Error::BigNumParsing(..) => AdapterError::adapter(err),
            err @ Error::SignMessage(..) => AdapterError::adapter(err),
            err @ Error::VerifyMessage(..) => AdapterError::adapter(err),
            err @ Error::ContractInitialization(..) => AdapterError::adapter(err),
            err @ Error::ContractQuerying(..) => AdapterError::adapter(err),
            err @ Error::VerifyAddress(..) => AdapterError::adapter(err),
            err @ Error::OutpaceError(..) => AdapterError::adapter(err),
            err @ Error::AuthenticationTokenNotIntendedForUs { .. } => {
                AdapterError::authentication(err)
            }
            err @ Error::InsufficientAuthorizationPrivilege { .. } => {
                AdapterError::authorization(err)
            }
        }
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("Keystore: {0}")]
    Keystore(#[from] KeystoreError),
    #[error("Wallet unlocking: {0}")]
    WalletUnlock(#[from] ethsign::Error),
    #[error("Web3: {0}")]
    Web3(#[from] web3::Error),
    /// When the ChannelId that we get from hashing the EthereumChannel with the contract address
    /// does not align with the provided Channel
    #[error("The hashed EthereumChannel.id ({expected}) is not the same as the Channel.id ({actual}) that was provided")]
    InvalidChannelId {
        expected: ChannelId,
        actual: ChannelId,
    },
    #[error("Channel ({0}) is not Active on the ethereum network")]
    ChannelInactive(ChannelId),
    /// Signing of the message failed
    #[error("Signing message: {0}")]
    SignMessage(#[from] EwtSigningError),
    #[error("Verifying message: {0}")]
    VerifyMessage(#[from] EwtVerifyError),
    #[error("Contract initialization: {0}")]
    ContractInitialization(web3::ethabi::Error),
    #[error("Contract querying: {0}")]
    ContractQuerying(web3::contract::Error),
    /// Error occurred during verification of Signature and/or StateRoot and/or Address
    #[error("Verifying address: {0}")]
    VerifyAddress(#[from] VerifyError),
    #[error("The intended {0:?} in the authentication token in not whitelisted")]
    ChainNotWhitelisted(ChainId),
    #[error("Deposit asset {0} is invalid")]
    InvalidDepositAsset(#[from] AddressError),
    #[error("Parsing BigNum: {0}")]
    BigNumParsing(#[from] ParseBigIntError),
    #[error("Token Payload.id({}) !== whoami({whoami}): token was not intended for us", .payload.id)]
    AuthenticationTokenNotIntendedForUs {
        payload: Payload,
        whoami: ValidatorId,
    },
    #[error("Insufficient privilege")]
    InsufficientAuthorizationPrivilege,
    #[error("Outpace contract error: {0}")]
    OutpaceError(#[from] OutpaceError),
}

#[derive(Debug, Error)]
/// Error returned on `eth_adapter.verify()` when the combination of
/// (signer, state_root, signature) **doesn't align**.
pub enum VerifyError {
    #[error("Recovering the public key from the signature: {0}")]
    /// `secp256k1` error
    PublicKeyRecovery(String),
    #[error("Decoding state root: {0}")]
    StateRootDecoding(#[source] hex::FromHexError),
    #[error("Decoding signature: {0}")]
    SignatureDecoding(#[source] hex::FromHexError),
    #[error("Signature is not prefixed with `0x`")]
    SignatureNotPrefixed,
    #[error("Signature length or V component of the Signature was incorrect")]
    SignatureInvalid,
}

#[derive(Debug, Error)]
pub enum KeystoreError {
    /// `address` key is missing from the keystore file
    #[error("\"address\" key missing in keystore file")]
    AddressMissing,
    /// The `address` key in the keystore file is not a valid `ValidatorId`
    #[error("\"address\" length should be 20 bytes")]
    AddressLength,
    /// reading the keystore file failed
    #[error("Reading keystore file: {0}")]
    ReadingFile(#[source] std::io::Error),
    /// Deserializing the keystore file failed
    #[error("Deserializing keystore file: {0}")]
    Deserialization(#[source] serde_json::Error),
}

#[derive(Debug, Error)]
pub enum EwtSigningError {
    #[error("Header serialization: {0}")]
    HeaderSerialization(#[source] serde_json::Error),
    #[error("Payload serialization: {0}")]
    PayloadSerialization(#[source] serde_json::Error),
    /// we must use a [`String`] since [`ethsign`] does not export the error from
    /// the `secp256k1` crate which is returned in the [`ethsign::SecretKey::sign()`] method.
    #[error("Signing message: {0}")]
    SigningMessage(String),
    #[error("Decoding hex of Signature: {0}")]
    DecodingHexSignature(#[from] hex::FromHexError),
}

#[derive(Debug, Error)]
pub enum OutpaceError {
    #[error("Error while signing outpace contract: {0}")]
    SignStateroot(String),
}

#[derive(Debug, Error)]
pub enum EwtVerifyError {
    #[error("The Ethereum Web Token header is invalid")]
    InvalidHeader,
    #[error("The token length should be at least 16 characters of length")]
    InvalidTokenLength,
    #[error("The token does not comply to the format of header.payload.signature")]
    InvalidToken,
    /// We use a `String` because [`ethsign::Signature::recover()`] returns `secp256k1` error
    /// which is not exported in `ethsign`.
    #[error("Address recovery: {0}")]
    AddressRecovery(String),
    #[error("Signature decoding: {0}")]
    SignatureDecoding(#[source] base64::DecodeError),
    /// If there is no suffix in the signature for the mode
    /// or if Signature length is not 65 bytes
    /// or if Signature V component is not in "Electrum" notation (`< 27`).
    #[error("Error when decoding token signature")]
    InvalidSignature,
    #[error("Payload error: {0}")]
    Payload(#[from] PayloadError),
}

#[derive(Debug, Error)]
pub enum PayloadError {
    #[error("Payload decoding: {0}")]
    Decoding(#[source] base64::DecodeError),
    #[error("Payload deserialization: {0}")]
    Deserialization(#[from] serde_json::Error),
    #[error("Payload is not a valid UTF-8 string: {0}")]
    Utf8(#[from] std::str::Utf8Error),
}

#[cfg(test)]
mod test {
    use super::*;

    fn assert_send<T: Send>() {}
    fn assert_sync<T: Sync>() {}

    #[test]
    fn a_correct_error() {
        // Ethereum adapter should be Send!
        assert_send::<Error>();
        // Ethereum adapter should be Sync!
        assert_sync::<Error>();
    }
}
