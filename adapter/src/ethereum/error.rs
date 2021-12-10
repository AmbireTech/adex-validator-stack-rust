use primitives::{
    address::Error as AddressError, big_num::ParseBigIntError, Address,
    ChannelId, ValidatorId,
};
use crate::Error as AdapterError;
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
            err @ Error::TokenNotWhitelisted(..) => AdapterError::adapter(err),
            err @ Error::InvalidDepositAsset(..) => AdapterError::adapter(err),
            err @ Error::BigNumParsing(..) => AdapterError::adapter(err),
            err @ Error::SignMessage(..) => AdapterError::adapter(err),
            err @ Error::VerifyMessage(..) => AdapterError::adapter(err),
            err @ Error::ContractInitialization(..) => AdapterError::adapter(err),
            err @ Error::ContractQuerying(..) => AdapterError::adapter(err),
            err @ Error::VerifyAddress(..) => AdapterError::adapter(err),
            err @ Error::AuthenticationTokenNotIntendedForUs { .. } => AdapterError::authentication(err),
            err @ Error::InsufficientAuthorizationPrivilege { .. } => AdapterError::authorization(err),
        }
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("Keystore: {0}")]
    Keystore(#[from] KeystoreError),
    #[error("Wallet unlocking: {0}")]
    /// Since [`ethstore::Error`] is not [`Sync`] we must use a [`String`] instead.
    WalletUnlock(String),
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
    #[error("Token not whitelisted: {0}")]
    TokenNotWhitelisted(Address),
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
}

#[derive(Debug, Error)]
/// Error returned on `eth_adapter.verify()` when the combination of
/// (signer, state_root, signature) **doesn't align**.
pub enum VerifyError {
    #[error("Recovering the public key from the signature: {0}")]
    PublicKeyRecovery(#[from] ethstore::ethkey::Error),
    #[error("Decoding state root: {0}")]
    StateRootDecoding(#[source] hex::FromHexError),
    #[error("Decoding signature: {0}")]
    SignatureDecoding(#[source] hex::FromHexError),
    #[error("Signature is not prefixed with `0x`")]
    SignatureNotPrefixed,
}

#[derive(Debug, Error)]
pub enum KeystoreError {
    /// `address` key is missing from the keystore file
    #[error("\"address\" key missing in keystore file")]
    AddressMissing,
    /// The `address` key in the keystore file is not a valid `ValidatorId`
    #[error("\"address\" is invalid: {0}")]
    AddressInvalid(#[source] primitives::address::Error),
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
    /// Since [`ethstore::Error`] is not [`Sync`] we must use a [`String`] instead.
    #[error("Signing message: {0}")]
    SigningMessage(String),
    #[error("Decoding hex of Signature: {0}")]
    DecodingHexSignature(#[from] hex::FromHexError),
}

#[derive(Debug, Error)]
pub enum EwtVerifyError {
    #[error("The Ethereum Web Token header is invalid")]
    InvalidHeader,
    #[error("The token length should be at least 16 characters of length")]
    InvalidTokenLength,
    #[error("The token does not comply to the format of header.payload.signature")]
    InvalidToken,
    #[error("Address recovery: {0}")]
    AddressRecovery(#[from] ethstore::ethkey::Error),
    #[error("Signature decoding: {0}")]
    SignatureDecoding(#[source] base64::DecodeError),
    /// When token is decoded but creating a Signature results in empty Signature.
    /// Signature is encoded as RSV (V in "Electrum" notation)
    /// See [`Signature::from_electrum`]
    #[error("Error when decoding token signature")]
    InvalidSignature,
    #[error("Payload decoding: {0}")]
    PayloadDecoding(#[source] base64::DecodeError),
    #[error("Payload deserialization: {0}")]
    PayloadDeserialization(#[from] serde_json::Error),
    #[error("Payload is not a valid UTF-8 string: {0}")]
    PayloadUtf8(#[from] std::str::Utf8Error),
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
