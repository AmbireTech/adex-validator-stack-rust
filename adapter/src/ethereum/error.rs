use primitives::adapter::{AdapterErrorKind, Error as AdapterError};
use primitives::ChannelId;
use std::fmt;

#[derive(Debug)]
pub enum Error {
    Keystore(KeystoreError),
    WalletUnlock(ethstore::Error),
    Web3(web3::Error),
    RelayerClient(reqwest::Error),
    /// When the ChannelId that we get from hashing the EthereumChannel with the contract address
    /// does not align with the provided Channel
    InvalidChannelId {
        expected: ChannelId,
        actual: ChannelId,
    },
    ChannelInactive(ChannelId),
    /// Signing of the message failed
    SignMessage(EwtSigningError),
    VerifyMessage(EwtVerifyError),
    ContractInitialization(web3::ethabi::Error),
    ContractQuerying(web3::contract::Error),
    /// Error occurred during verification of Signature and/or StateRoot and/or Address
    VerifyAddress(VerifyError),
}

impl std::error::Error for Error {}

impl AdapterErrorKind for Error {}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Error::*;

        match self {
                Keystore(err) => write!(f, "Keystore: {}", err),
                WalletUnlock(err) => write!(f, "Wallet unlocking: {}", err),
                Web3(err) => write!(f, "Web3: {}", err),
                RelayerClient(err) => write!(f, "Relayer client: {}", err),
                InvalidChannelId { expected, actual} => write!(f, "The hashed EthereumChannel.id ({}) is not the same as the Channel.id ({}) that was provided", expected, actual),
                ChannelInactive(channel_id) => write!(f, "Channel ({}) is not Active on the ethereum network", channel_id),
                SignMessage(err) => write!(f, "Signing message: {}", err),
                VerifyMessage(err) => write!(f, "Verifying message: {}", err),
                ContractInitialization(err) => write!(f, "Contract initialization: {}", err),
                ContractQuerying(err) => write!(f, "Contract querying: {}", err),
                VerifyAddress(err) => write!(f, "Verifying address: {}", err)
            }
    }
}

#[derive(Debug)]
/// Error returned on `eth_adapter.verify()` when the combination of
/// (signer, state_root, signature) **doesn't align**.
pub enum VerifyError {
    PublicKeyRecovery(ethstore::ethkey::Error),
    StateRootDecoding(hex::FromHexError),
    SignatureDecoding(hex::FromHexError),
    SignatureNotPrefixed,
}

impl fmt::Display for VerifyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use VerifyError::*;

        match self {
            PublicKeyRecovery(err) => {
                write!(f, "Recovering the public key from the signature: {}", err)
            }
            StateRootDecoding(err) => write!(f, "Decoding state root: {}", err),
            SignatureDecoding(err) => write!(f, "Decoding signature: {}", err),
            SignatureNotPrefixed => write!(f, "Signature is not prefixed with `0x`"),
        }
    }
}

impl From<VerifyError> for AdapterError<Error> {
    fn from(err: VerifyError) -> Self {
        AdapterError::Adapter(Error::VerifyAddress(err).into())
    }
}

#[derive(Debug)]
pub enum KeystoreError {
    /// `address` key is missing from the keystore file
    AddressMissing,
    /// The `address` key in the keystore file is not a valid `ValidatorId`
    AddressInvalid(primitives::DomainError),
    /// reading the keystore file failed
    ReadingFile(std::io::Error),
    /// Deserializing the keystore file failed
    Deserialization(serde_json::Error),
}

impl std::error::Error for KeystoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        use KeystoreError::*;
        match self {
            AddressMissing => None,
            AddressInvalid(err) => Some(err),
            ReadingFile(err) => Some(err),
            Deserialization(err) => Some(err),
        }
    }
}

impl fmt::Display for KeystoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use KeystoreError::*;

        match self {
            AddressMissing => write!(f, "\"address\" key missing in keystore file"),
            AddressInvalid(err) => write!(f, "\"address\" is invalid: {}", err),
            ReadingFile(err) => write!(f, "Reading keystore file: {}", err),
            Deserialization(err) => write!(f, "Deserializing keystore file: {}", err),
        }
    }
}

impl From<KeystoreError> for AdapterError<Error> {
    fn from(err: KeystoreError) -> Self {
        AdapterError::Adapter(Error::Keystore(err).into())
    }
}

#[derive(Debug)]
pub enum EwtSigningError {
    HeaderSerialization(serde_json::Error),
    PayloadSerialization(serde_json::Error),
    SigningMessage(ethstore::Error),
    DecodingHexSignature(hex::FromHexError),
}

impl fmt::Display for EwtSigningError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use EwtSigningError::*;

        match self {
            HeaderSerialization(err) => write!(f, "Header serialization: {}", err),
            PayloadSerialization(err) => write!(f, "Payload serialization: {}", err),
            SigningMessage(err) => write!(f, "Signing message: {}", err),
            DecodingHexSignature(err) => write!(f, "Decoding hex of Signature: {}", err),
        }
    }
}
impl From<EwtSigningError> for AdapterError<Error> {
    fn from(err: EwtSigningError) -> Self {
        AdapterError::Adapter(Error::SignMessage(err).into())
    }
}

#[derive(Debug)]
pub enum EwtVerifyError {
    AddressRecovery(ethstore::ethkey::Error),
    SignatureDecoding(base64::DecodeError),
    PayloadDecoding(base64::DecodeError),
    PayloadDeserialization(serde_json::Error),
    PayloadUtf8(std::string::FromUtf8Error),
}

impl fmt::Display for EwtVerifyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use EwtVerifyError::*;

        match self {
            AddressRecovery(err) => write!(f, "Address recovery: {}", err),
            SignatureDecoding(err) => write!(f, "Signature decoding: {}", err),
            PayloadDecoding(err) => write!(f, "Payload decoding: {}", err),
            PayloadDeserialization(err) => write!(f, "Payload deserialization: {}", err),
            PayloadUtf8(err) => write!(f, "Payload is not a valid utf8 string: {}", err),
        }
    }
}
