use std::fmt;

use ethstore::{
    ethkey::{public_to_address, recover, Message, Password, Signature},
    SafeAccount,
};
use once_cell::sync::Lazy;
use primitives::{Address, ValidatorId};
use serde::{Deserialize, Serialize};
use web3::signing::keccak256;

use super::{
    error::{EwtSigningError, EwtVerifyError},
    to_ethereum_signed,
};

pub static ETH_SIGN_SUFFIX: Lazy<Vec<u8>> = Lazy::new(|| hex::decode("01").unwrap());

pub static ETH_HEADER: Lazy<Header> = Lazy::new(|| Header {
    header_type: "JWT".to_string(),
    alg: "ETH".to_string(),
});

pub static ETH_HEADER_BASE64: Lazy<String> =
    Lazy::new(|| base64_encode(&*ETH_HEADER).expect("Header should be serializable"));

/// Serializes the value into a JSON and then it encodes the result using `base64`
/// Base64 encoding is performed using the [`base64::URL_SAFE_NO_PAD`] configuration
fn base64_encode<T: Serialize>(value: &T) -> Result<String, serde_json::Error> {
    let json = serde_json::to_string(value)?;

    Ok(base64::encode_config(&json, base64::URL_SAFE_NO_PAD))
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct Header {
    /// `typ` is the type of the token
    /// `EWT` for ethereum
    #[serde(rename = "typ")]
    header_type: String,
    alg: String,
}

/// The [`Payload`] of the Ethereum Web Token
///
/// All addresses should be `0x` prefixed & checkesumed
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Payload {
    /// The intended Validator Id for which the token is/should be created.
    pub id: ValidatorId,
    pub era: i64,
    pub address: Address,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<Address>,
}

impl Payload {
    /// Decodes the [`Payload`] from a base64 encoded json string
    // TODO: replace with own error?
    pub fn base64_decode(encoded_json: &str) -> Result<Self, EwtVerifyError> {
        let base64_decode = base64::decode_config(encoded_json, base64::URL_SAFE_NO_PAD)
            .map_err(EwtVerifyError::PayloadDecoding)?;

        let json = std::str::from_utf8(&base64_decode).map_err(EwtVerifyError::PayloadUtf8)?;

        serde_json::from_str(json).map_err(EwtVerifyError::PayloadDeserialization)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
/// EWT verified payload
pub struct VerifyPayload {
    /// The signer of the token who's been verified
    pub from: Address,
    /// The payload that has been verified
    pub payload: Payload,
}

/// EWT Authentication Token
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub header: Header,
    pub payload: Payload,
    /// The hashed value of the message:
    /// `keccak256(header_json_base64.payload_json_base64)`
    pub message_hash: [u8; 32],
    /// The signature after signing the message `to_ethereum_signed(keccak256("{header_base64}.{payload_base64}"))`
    /// The signature is in the form of `{r}{s}{v}{mode}` where `mode` is `01` for Ethereum Signature
    pub signature: Vec<u8>,
    /// Will result in authentication token string in the format of:
    /// `{header_base64_encoded}.{payload_base64_encoded}.{signature_base64_encoded}`
    /// All fields are base64 encoded
    pub token: String,
}

impl Token {
    /// Signs a payload given a signer account & password.
    /// For the [`Header`] it uses [`ETH_HEADER`].
    ///
    /// `Ethereum web token` signing of payload with 1 difference:
    /// it's not the JSON string representation that gets signed
    /// but the `keccak256(payload_json)`.
    pub fn sign(
        signer: &SafeAccount,
        password: &Password,
        payload: Payload,
    ) -> Result<Self, EwtSigningError> {
        let header = ETH_HEADER.clone();
        let header_encoded =
            base64_encode(&header).map_err(EwtSigningError::HeaderSerialization)?;

        let payload_encoded =
            base64_encode(&payload).map_err(EwtSigningError::PayloadSerialization)?;

        let message_hash = keccak256(format!("{}.{}", header_encoded, payload_encoded).as_bytes());

        // the singed message should be conform to the "Signed Data Standard"
        let message_to_sign = Message::from(to_ethereum_signed(&message_hash));

        let mut signature = signer
            .sign(password, &message_to_sign)
            .map_err(EwtSigningError::SigningMessage)?
            .into_electrum()
            .to_vec();
        signature.extend(ETH_SIGN_SUFFIX.as_slice());

        let signature_encoded = base64::encode_config(&signature, base64::URL_SAFE_NO_PAD);

        Ok(Self {
            header,
            payload,
            message_hash,
            signature,
            token: format!(
                "{}.{}.{}",
                header_encoded, payload_encoded, signature_encoded
            ),
        })
    }

    pub fn verify(token: &str) -> Result<(Token, VerifyPayload), EwtVerifyError> {
        if token.len() < 16 {
            return Err(EwtVerifyError::InvalidTokenLength);
        }

        let token_parts = token.splitn(3, '.').collect::<Vec<_>>();
        let ((header_encoded, payload_encoded), signature_encoded) = token_parts
            .get(0)
            .zip(token_parts.get(1))
            .zip(token_parts.get(2))
            .ok_or(EwtVerifyError::InvalidToken)?;

        // if the encoded value of the header matches the expected one
        // we have a valid EWT header
        let header = if header_encoded == &*ETH_HEADER_BASE64 {
            ETH_HEADER.clone()
        } else {
            return Err(EwtVerifyError::InvalidHeader);
        };

        let payload = Payload::base64_decode(payload_encoded)?;

        let decoded_signature = base64::decode_config(&signature_encoded, base64::URL_SAFE_NO_PAD)
            .map_err(EwtVerifyError::SignatureDecoding)?;

        // if it returns the same slice, then there was no suffix
        // `01` suffix is the Ethereum Signature
        let stripped_signature = match decoded_signature.strip_suffix(ETH_SIGN_SUFFIX.as_slice()) {
            // we have a valid signature only if a suffix **was removed**
            Some(stripped_signature) if stripped_signature != decoded_signature => {
                Ok(stripped_signature)
            }
            _ => Err(EwtVerifyError::InvalidSignature),
        }?;

        let signature = Signature::from_electrum(stripped_signature);
        // signature is empty, so it is invalid, see `Signature::from_electrum`
        if signature == Signature::default() {
            return Err(EwtVerifyError::InvalidSignature);
        }

        let message_hash = keccak256(format!("{}.{}", header_encoded, payload_encoded).as_bytes());

        let recover_message = Message::from(to_ethereum_signed(&message_hash));
        let address = public_to_address(
            &recover(&signature, &recover_message).map_err(EwtVerifyError::AddressRecovery)?,
        );

        let token = Token {
            header,
            payload: payload.clone(),
            message_hash,
            signature: decoded_signature,
            token: token.to_string(),
        };

        let verified_payload = VerifyPayload {
            from: Address::from(address.0),
            payload,
        };

        Ok((token, verified_payload))
    }

    /// Returns the EWT Token string ready to be used in an Authentication header.
    pub fn as_str(&self) -> &str {
        self.token.as_str()
    }
}

/// The EWT Token string is returned, ready to be used in an Authentication header.
impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.token)
    }
}

#[cfg(test)]
mod test {
    use primitives::{
        adapter::Adapter,
        config::GANACHE_CONFIG,
        test_util::{CREATOR, LEADER},
        ValidatorId,
    };

    use super::*;
    use crate::{ethereum::test_util::KEYSTORES, EthereumAdapter};

    #[test]
    fn should_generate_correct_ewt_sign_and_verify() {
        let mut eth_adapter = EthereumAdapter::init(KEYSTORES[&CREATOR].clone(), &GANACHE_CONFIG)
            .expect("should init ethereum adapter");

        eth_adapter.unlock().expect("should unlock eth adapter");

        let payload = Payload {
            id: ValidatorId::from(*LEADER),
            era: 100_000,
            address: eth_adapter.whoami().to_address(),
            identity: None,
        };
        let wallet = eth_adapter.wallet.clone().expect("Unlocked adapter");
        let token = Token::sign(&wallet, &eth_adapter.keystore_pwd, payload)
            .expect("failed to generate ewt signature");
        let expected = "eyJ0eXAiOiJKV1QiLCJhbGciOiJFVEgifQ.eyJpZCI6IjB4ODA2OTA3NTE5NjlCMjM0Njk3ZTkwNTllMDRlZDcyMTk1YzM1MDdmYSIsImVyYSI6MTAwMDAwLCJhZGRyZXNzIjoiMHhhQ0JhREEyZDU4MzBkMTg3NWFlM0QyZGUyMDdBMTM2M0IzMTZEZjJGIn0.HVZ3qD2pdY_dqgNgJZTB7vhkpKBmMDzQ1tigee1aSd0ugnA_4D12nilJtpfS0KcG7soAMRqwCXw0-1hUqDqUrxsB";
        assert_eq!(token.as_str(), expected, "generated wrong ewt signature");

        let expected_verification_response = VerifyPayload {
            from: *CREATOR,
            payload: Payload {
                id: ValidatorId::from(*LEADER),
                era: 100_000,
                address: *CREATOR,
                identity: None,
            },
        };

        let (verified_token, verification) =
            Token::verify(expected).expect("Failed to verify ewt token");

        assert_eq!(verified_token, token);
        assert_eq!(
            expected_verification_response, verification,
            "generated wrong verification payload"
        );
    }
}
