use async_trait::async_trait;
use chrono::Utc;
use create2::calc_addr;
use error::*;
use ethstore::{
    ethkey::{verify_address, Message, Password, Signature},
    SafeAccount,
};
use once_cell::sync::Lazy;
use primitives::{
    adapter::{Adapter, AdapterResult, Deposit, Error as AdapterError, KeystoreOptions, Session},
    config::Config,
    Address, BigNum, Channel, ValidatorId,
};
use serde_json::Value;
use std::{fs, str::FromStr};
use web3::{
    contract::{Contract, Options},
    ethabi::{encode, Token},
    signing::keccak256,
    transports::Http,
    types::{H160, U256},
    Web3,
};

use self::ewt::Payload;

mod error;
/// Ethereum Web Token
///
/// This module implements the Ethereum Web Token with 2 difference:
/// - The signature includes the Ethereum signature mode, see [`ETH_SIGN_SUFFIX`]
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

pub fn get_counterfactual_address(
    sweeper: Address,
    channel: &Channel,
    outpace: Address,
    depositor: Address,
) -> Address {
    let salt: [u8; 32] = [0; 32];
    let encoded_params = encode(&[
        Token::Address(outpace.as_bytes().into()),
        channel.tokenize(),
        Token::Address(depositor.as_bytes().into()),
    ]);

    let mut init_code = DEPOSITOR_BYTECODE_DECODED.clone();
    init_code.extend(&encoded_params);

    let address_bytes = calc_addr(sweeper.as_bytes(), &salt, &init_code);

    Address::from(address_bytes)
}

trait EthereumChannel {
    fn tokenize(&self) -> Token;
}

impl EthereumChannel for Channel {
    fn tokenize(&self) -> Token {
        let tokens = vec![
            Token::Address(self.leader.as_bytes().into()),
            Token::Address(self.follower.as_bytes().into()),
            Token::Address(self.guardian.as_bytes().into()),
            Token::Address(self.token.as_bytes().into()),
            Token::FixedBytes(self.nonce.to_bytes().to_vec()),
        ];

        Token::Tuple(tokens)
    }
}

#[derive(Debug, Clone)]
pub struct EthereumAdapter {
    address: ValidatorId,
    keystore_json: Value,
    keystore_pwd: Password,
    config: Config,
    wallet: Option<SafeAccount>,
    web3: Web3<Http>,
}

impl EthereumAdapter {
    pub fn init(opts: KeystoreOptions, config: &Config) -> AdapterResult<EthereumAdapter, Error> {
        let keystore_contents =
            fs::read_to_string(&opts.keystore_file).map_err(KeystoreError::ReadingFile)?;
        let keystore_json: Value =
            serde_json::from_str(&keystore_contents).map_err(KeystoreError::Deserialization)?;

        let address = {
            let keystore_address = keystore_json
                .get("address")
                .and_then(|value| value.as_str())
                .ok_or(KeystoreError::AddressMissing)?;

            keystore_address
                .parse()
                .map_err(KeystoreError::AddressInvalid)
        }?;

        let transport =
            web3::transports::Http::new(&config.ethereum_network).map_err(Error::Web3)?;
        let web3 = web3::Web3::new(transport);

        Ok(Self {
            address,
            keystore_json,
            keystore_pwd: opts.keystore_pwd.into(),
            wallet: None,
            config: config.to_owned(),
            web3,
        })
    }

    /// Checks if the signer of the `hash` & `signature` has privileges,
    /// by using the Identity contract and the passed identity [`Address`]
    /// See https://eips.ethereum.org/EIPS/eip-1271
    /// Signature should be `01` suffixed for Eth Sign
    pub async fn has_privileges(
        &self,
        identity: Address,
        hash: [u8; 32],
        signature_with_mode: &[u8],
    ) -> Result<bool, Error> {
        // 0x1626ba7e is in little endian
        let magic_value: u32 = 2126128662;
        // u32::MAX = 4294967295
        let _no_access_value: u32 = 0xffffffff;

        let identity_contract =
            Contract::from_json(self.web3.eth(), H160(identity.to_bytes()), &IDENTITY_ABI)
                .map_err(Error::ContractInitialization)?;

        // we receive `bytes4` from the contract
        let status: [u8; 4] = identity_contract
            .query(
                "isValidSignature",
                (
                    // bytes32
                    Token::FixedBytes(hash.to_vec()),
                    // bytes
                    Token::Bytes(signature_with_mode.to_vec()),
                ),
                None,
                Options::default(),
                None,
            )
            .await
            .map_err(Error::ContractQuerying)?;

        // turn this into `u32` (EVM is in little endian)
        let actual_value = u32::from_le_bytes(status);

        // if it is the magical value then the address has privileges.
        Ok(actual_value == magic_value)
    }
}

#[async_trait]
impl Adapter for EthereumAdapter {
    type AdapterError = Error;

    fn unlock(&mut self) -> AdapterResult<(), Self::AdapterError> {
        let json = serde_json::from_value(self.keystore_json.clone())
            .map_err(KeystoreError::Deserialization)?;

        let account = SafeAccount::from_file(json, None, &Some(self.keystore_pwd.clone()))
            .map_err(Error::WalletUnlock)?;

        self.wallet = Some(account);

        Ok(())
    }

    fn whoami(&self) -> ValidatorId {
        self.address
    }

    fn sign(&self, state_root: &str) -> AdapterResult<String, Self::AdapterError> {
        if let Some(wallet) = &self.wallet {
            let state_root = hex::decode(state_root).map_err(VerifyError::StateRootDecoding)?;
            let message = Message::from(to_ethereum_signed(&state_root));
            let wallet_sign = wallet
                .sign(&self.keystore_pwd, &message)
                .map_err(EwtSigningError::SigningMessage)?;
            let signature: Signature = wallet_sign.into_electrum().into();

            Ok(format!("0x{}", signature))
        } else {
            Err(AdapterError::LockedWallet)
        }
    }

    /// `state_root` is hex string which **should not** be `0x` prefixed
    /// `sig` is hex string which **should be** `0x` prefixed
    fn verify(
        &self,
        signer: ValidatorId,
        state_root: &str,
        sig: &str,
    ) -> AdapterResult<bool, Self::AdapterError> {
        if !sig.starts_with("0x") {
            return Err(VerifyError::SignatureNotPrefixed.into());
        }
        let decoded_signature = hex::decode(&sig[2..]).map_err(VerifyError::SignatureDecoding)?;
        let address = ethstore::ethkey::Address::from(*signer.as_bytes());
        let signature = Signature::from_electrum(&decoded_signature);
        let state_root = hex::decode(state_root).map_err(VerifyError::StateRootDecoding)?;
        let message = Message::from(to_ethereum_signed(&state_root));

        let verify_address = verify_address(&address, &signature, &message)
            .map_err(VerifyError::PublicKeyRecovery)?;

        Ok(verify_address)
    }

    /// Creates a `Session` from a provided Token by calling the Contract.
    /// Does **not** cache the (`Token`, `Session`) pair.
    async fn session_from_token<'a>(
        &'a self,
        token: &'a str,
    ) -> AdapterResult<Session, Self::AdapterError> {
        let (verified_token, verified) = ewt::Token::verify(token).map_err(Error::VerifyMessage)?;

        if self.whoami() != verified.payload.id {
            return Err(AdapterError::Authentication(
                "token payload.id !== whoami(): token was not intended for us".to_string(),
            ));
        }

        let sess = match &verified.payload.identity {
            Some(identity) => {
                // the Hash for has_privileges should **not** be an Ethereum Signed Message hash

                if self
                    .has_privileges(
                        *identity,
                        verified_token.message_hash,
                        &verified_token.signature,
                    )
                    .await?
                {
                    Session {
                        era: verified.payload.era,
                        uid: identity.to_owned(),
                    }
                } else {
                    return Err(AdapterError::Authorization(
                        "insufficient privilege".to_string(),
                    ));
                }
            }
            None => Session {
                era: verified.payload.era,
                uid: verified.from,
            },
        };

        Ok(sess)
    }

    fn get_auth(&self, intended_for: &ValidatorId) -> AdapterResult<String, Self::AdapterError> {
        let wallet = self.wallet.as_ref().ok_or(AdapterError::LockedWallet)?;

        let era = Utc::now().timestamp_millis() as f64 / 60000.0;
        let payload = Payload {
            id: *intended_for,
            era: era.floor() as i64,
            identity: None,
            address: self.whoami().to_address(),
        };

        let token = ewt::Token::sign(wallet, &self.keystore_pwd, payload)
            .map_err(|err| AdapterError::Adapter(Error::SignMessage(err).into()))?;

        Ok(token.to_string())
    }

    async fn get_deposit(
        &self,
        channel: &Channel,
        depositor_address: &Address,
    ) -> AdapterResult<Deposit, Self::AdapterError> {
        let token_info = self
            .config
            .token_address_whitelist
            .get(&channel.token)
            .ok_or(Error::TokenNotWhitelisted(channel.token))?;

        let outpace_contract = Contract::from_json(
            self.web3.eth(),
            self.config.outpace_address.into(),
            &OUTPACE_ABI,
        )
        .map_err(Error::ContractInitialization)?;

        let erc20_contract =
            Contract::from_json(self.web3.eth(), channel.token.as_bytes().into(), &ERC20_ABI)
                .map_err(Error::ContractInitialization)?;

        let sweeper_contract = Contract::from_json(
            self.web3.eth(),
            self.config.sweeper_address.into(),
            &SWEEPER_ABI,
        )
        .map_err(Error::ContractInitialization)?;

        let sweeper_address = Address::from(sweeper_contract.address().to_fixed_bytes());
        let outpace_address = Address::from(outpace_contract.address().to_fixed_bytes());

        let on_outpace: U256 = outpace_contract
            .query(
                "deposits",
                (
                    Token::FixedBytes(channel.id().as_bytes().to_vec()),
                    Token::Address(H160(depositor_address.to_bytes())),
                ),
                None,
                Options::default(),
                None,
            )
            .await
            .map_err(Error::ContractQuerying)?;

        let on_outpace = BigNum::from_str(&on_outpace.to_string()).map_err(Error::BigNumParsing)?;

        let counterfactual_address = get_counterfactual_address(
            sweeper_address,
            channel,
            outpace_address,
            *depositor_address,
        );
        let still_on_create2: U256 = erc20_contract
            .query(
                "balanceOf",
                H160(counterfactual_address.to_bytes()),
                None,
                Options::default(),
                None,
            )
            .await
            .map_err(Error::ContractQuerying)?;

        let still_on_create2: BigNum = still_on_create2
            .to_string()
            .parse()
            .map_err(Error::BigNumParsing)?;

        // Count the create2 deposit only if it's > minimum token units configured
        let deposit = if still_on_create2 > token_info.min_token_units_for_deposit {
            Deposit {
                total: &still_on_create2 + &on_outpace,
                still_on_create2,
            }
        } else {
            Deposit {
                total: on_outpace,
                still_on_create2: BigNum::from(0),
            }
        };

        Ok(deposit)
    }
}

#[cfg(test)]
mod test {
    use super::{ewt::ETH_SIGN_SUFFIX, test_util::*, *};
    use chrono::Utc;
    use primitives::{
        config::{DEVELOPMENT_CONFIG, GANACHE_CONFIG},
        test_util::{ADDRESS_3, ADDRESS_4, ADDRESS_5, ADVERTISER, CREATOR, IDS, LEADER},
        ToHex,
    };
    use web3::{transports::Http, Web3};

    #[test]
    fn should_init_and_unlock_ethereum_adapter() {
        let mut eth_adapter = setup_eth_adapter(DEVELOPMENT_CONFIG.clone());
        eth_adapter.unlock().expect("should unlock eth adapter");
    }

    #[test]
    fn should_get_whoami_sign_and_verify_messages() {
        // whoami
        let mut eth_adapter = setup_eth_adapter(DEVELOPMENT_CONFIG.clone());
        let whoami = eth_adapter.whoami();
        assert_eq!(
            whoami.to_string(),
            "0x2bDeAFAE53940669DaA6F519373f686c1f3d3393",
            "failed to get correct whoami"
        );

        eth_adapter.unlock().expect("should unlock eth adapter");

        // Sign
        let expected_response =
            "0x625fd46f82c4cfd135ea6a8534e85dbf50beb157046dce59d2e97aacdf4e38381d1513c0e6f002b2f05c05458038b187754ff38cc0658dfc9ba854cccfb6e13e1b";
        let message = "2bdeafae53940669daa6f519373f686c";
        let signature = eth_adapter.sign(message).expect("failed to sign message");
        assert_eq!(expected_response, signature, "invalid signature");

        // Verify
        let signature =
            "0x9e07f12958ce7c5eb1362eb9461e4745dd9d74a42b921391393caea700bfbd6e1ad876a7d8f9202ef1fe6110dbfe87840c5676ca5c4fda9f3330694a1ac2a1fc1b";
        let verify = eth_adapter
            .verify(
                ValidatorId::try_from("2892f6C41E0718eeeDd49D98D648C789668cA67d")
                    .expect("Failed to parse id"),
                "8bc45d8eb27f4c98cab35d17b0baecc2a263d6831ef0800f4c190cbfac6d20a3",
                signature,
            )
            .expect("Failed to verify signatures");

        let signature2 = "0x9fa5852041b9818021323aff8260624fd6998c52c95d9ad5036e0db6f2bf2b2d48a188ec1d638581ff56b0a2ecceca6d3880fc65030558bd8f68b154e7ebf80f1b";
        let message2 = "1648231285e69677531ffe70719f67a07f3d4393b8425a5a1c84b0c72434c77b";

        let verify2 = eth_adapter
            .verify(IDS["leader"], message2, signature2)
            .expect("Failed to verify signatures");

        assert!(verify, "invalid signature 1 verification");
        assert!(verify2, "invalid signature 2 verification");
    }

    /// Validated using `lib/protocol-eth/js/Bundle.js`
    #[tokio::test]
    async fn test_has_privileges_with_raw_data() {
        let user = *ADDRESS_4;
        // the web3 from Adapter is used to deploy the Identity contract
        let mut user_adapter = EthereumAdapter::init(KEYSTORES[&user].clone(), &GANACHE_CONFIG)
            .expect("should init ethereum adapter");
        user_adapter.unlock().expect("should unlock eth adapter");

        let evil = *ADDRESS_5;
        let identify_as = *ADDRESS_3;

        let msg_hash_actual = keccak256(&hex::decode("21851b").unwrap());

        let msg_hash = "978d98785935b5526480e9ca00d01b063a6c56f51afbc4ad27b28daecb14258d";
        let msg_hash_decoded = hex::decode(msg_hash).unwrap();
        assert_eq!(msg_hash_decoded.len(), 32);
        assert_eq!(&hex::encode(msg_hash_actual), msg_hash);
        assert_eq!(&msg_hash_decoded, &msg_hash_actual);

        let (identity_address, _contract) =
            deploy_identity_contract(&user_adapter.web3, identify_as, &[user])
                .await
                .expect("Should deploy identity");

        // User should have privileges!
        {
            let user_sig = "0x41fb9082f8d369256ed7f46afff32efe1511128b92d0be3b2457f012e389320f348700a58395d8f55836cf5d95db431b1061d30b11114d21bd0c5d9dcd4791e71b01";

            let wallet = user_adapter.wallet.clone().unwrap();

            let signature_actual = {
                let ethers_sign_message = to_ethereum_signed(&msg_hash_actual);
                let message = Message::from(ethers_sign_message);

                let mut signature = wallet
                    .sign(&user_adapter.keystore_pwd, &message)
                    .expect("Should sign message")
                    .into_electrum()
                    .to_vec();

                signature.extend(ETH_SIGN_SUFFIX.as_slice());
                signature
            };

            assert_eq!(user_sig, signature_actual.to_hex_prefixed());

            let has_privileges = user_adapter
                .has_privileges(identity_address, msg_hash_actual, &signature_actual)
                .await
                .expect("Should get privileges");

            assert!(has_privileges, "User should have privileges!")
        }

        // Evil should **not** have privileges!
        {
            let evil_sig = "0x78b1d955c38baa3d98e11c5ed379191880d9f85acde31a1b7da1436825f688ec1b0a12bc749701a0135643dfb7f84a65697afa8065f886edb259f500651e415b1b01";

            let mut evil_adapter = EthereumAdapter::init(KEYSTORES[&evil].clone(), &GANACHE_CONFIG)
                .expect("should init ethereum adapter");
            evil_adapter.unlock().expect("should unlock eth adapter");
            let wallet = evil_adapter.wallet.clone().unwrap();

            let signature_actual = {
                let ethers_sign_message = to_ethereum_signed(&msg_hash_actual);
                let message = Message::from(ethers_sign_message);

                let mut signature = wallet
                    .sign(&evil_adapter.keystore_pwd, &message)
                    .expect("Should sign message")
                    .into_electrum()
                    .to_vec();

                signature.extend(ETH_SIGN_SUFFIX.as_slice());
                signature
            };

            assert_eq!(evil_sig, signature_actual.to_hex_prefixed());

            let has_privileges = evil_adapter
                .has_privileges(identity_address, msg_hash_actual, &signature_actual)
                .await
                .expect("Should get privileges");

            assert!(!has_privileges, "Evil should not have privileges!")
        }
    }

    #[tokio::test]
    async fn test_has_privileges_with_payload() {
        let mut adapter = EthereumAdapter::init(KEYSTORES[&LEADER].clone(), &GANACHE_CONFIG)
            .expect("should init ethereum adapter");
        adapter.unlock().expect("should unlock eth adapter");

        let whoami = adapter.whoami().to_address();
        assert_eq!(
            *LEADER, whoami,
            "Ethereum address should be authenticated with keystore file as LEADER!"
        );

        let (identity_address, contract) =
            deploy_identity_contract(&adapter.web3, *CREATOR, &[whoami])
                .await
                .expect("Should deploy identity");

        let set_privileges: [u8; 32] = contract
            .query(
                "privileges",
                Token::Address(H160(whoami.to_bytes())),
                None,
                Options::default(),
                None,
            )
            .await
            .expect("should query contract privileges");

        let expected_privileges = {
            let mut bytes32 = [0_u8; 32];
            bytes32[31] = 1;
            bytes32
        };
        assert_eq!(
            expected_privileges, set_privileges,
            "The Privilege set through constructor should be `1`"
        );

        let wallet = adapter.wallet.clone().expect("Should have unlocked wallet");

        let era = Utc::now().timestamp_millis() as f64 / 60000.0;
        let payload = Payload {
            id: adapter.whoami(),
            era: era.floor() as i64,
            address: adapter.whoami().to_address(),
            identity: Some(identity_address),
        };

        let auth_token = ewt::Token::sign(&wallet, &adapter.keystore_pwd, payload)
            .expect("Should sign successfully the Payload");

        let has_privileges = adapter
            .has_privileges(
                identity_address,
                auth_token.message_hash,
                &auth_token.signature,
            )
            .await
            .expect("Should get privileges");

        assert!(has_privileges);
    }

    #[tokio::test]
    async fn test_session_from_token() {
        let mut adapter = EthereumAdapter::init(KEYSTORES[&LEADER].clone(), &GANACHE_CONFIG)
            .expect("should init Leader ethereum adapter");
        adapter.unlock().expect("should unlock eth adapter");

        let (identity_address, _contract) =
            deploy_identity_contract(&adapter.web3, *CREATOR, &[*ADVERTISER])
                .await
                .expect("Should deploy identity");

        let mut signer_adapter =
            EthereumAdapter::init(KEYSTORES[&ADVERTISER].clone(), &GANACHE_CONFIG)
                .expect("should init Advertiser ethereum adapter");
        signer_adapter.unlock().expect("Should unlock eth adapter");

        assert_eq!(signer_adapter.whoami(), ValidatorId::from(*ADVERTISER));

        let signer_wallet = signer_adapter
            .wallet
            .clone()
            .expect("Should have unlocked wallet");

        let era = Utc::now().timestamp_millis() as f64 / 60000.0;
        let payload = Payload {
            // the intended ValidatorId for whom the payload is.
            id: adapter.whoami(),
            era: era.floor() as i64,
            identity: Some(identity_address),
            // The singer address
            address: signer_adapter.whoami().to_address(),
        };

        let token = ewt::Token::sign(&signer_wallet, &signer_adapter.keystore_pwd, payload)
            .expect("Should sign successfully the Payload");

        // double check that we have privileges for _Who Am I_
        assert!(adapter
            .has_privileges(identity_address, token.message_hash, &token.signature)
            .await
            .expect("Ok"));

        let session: Session = adapter.session_from_token(token.as_str()).await.unwrap();
        assert_eq!(session.uid, identity_address);
    }

    #[tokio::test]
    async fn get_deposit_and_count_create2_when_min_tokens_received() {
        let web3 = Web3::new(Http::new(GANACHE_URL).expect("failed to init transport"));

        let leader_account = *LEADER;

        // deploy contracts
        let token = deploy_token_contract(&web3, 1_000)
            .await
            .expect("Correct parameters are passed to the Token constructor.");
        let token_address = token.1;

        let sweeper = deploy_sweeper_contract(&web3)
            .await
            .expect("Correct parameters are passed to the Sweeper constructor.");

        let outpace = deploy_outpace_contract(&web3)
            .await
            .expect("Correct parameters are passed to the OUTPACE constructor.");

        let spender = *CREATOR;

        let channel = get_test_channel(token_address);

        let mut config = DEVELOPMENT_CONFIG.clone();
        config.sweeper_address = sweeper.0.to_bytes();
        config.outpace_address = outpace.0.to_bytes();
        // since we deploy a new contract, it's should be different from all the ones found in config.
        assert!(
            config
                .token_address_whitelist
                .insert(token_address, token.0)
                .is_none(),
            "Should not have previous value, we've just deployed the contract."
        );
        let mut eth_adapter = setup_eth_adapter(config);
        eth_adapter.unlock().expect("should unlock eth adapter");

        let counterfactual_address =
            get_counterfactual_address(sweeper.0, &channel, outpace.0, spender);

        // No Regular nor Create2 deposits
        {
            let no_deposits = eth_adapter
                .get_deposit(&channel, &spender)
                .await
                .expect("should get deposit");

            assert_eq!(
                Deposit {
                    total: BigNum::from(0),
                    still_on_create2: BigNum::from(0),
                },
                no_deposits
            );
        }

        // Regular deposit in Outpace without Create2
        {
            mock_set_balance(
                &token.2,
                *LEADER.as_bytes(),
                *spender.as_bytes(),
                &BigNum::from(10_000),
            )
            .await
            .expect("Failed to set balance");

            outpace_deposit(
                &outpace.1,
                &channel,
                *spender.as_bytes(),
                &BigNum::from(10_000),
            )
            .await
            .expect("Should deposit funds");

            let regular_deposit = eth_adapter
                .get_deposit(&channel, &spender)
                .await
                .expect("should get deposit");

            assert_eq!(
                Deposit {
                    total: BigNum::from(10_000),
                    still_on_create2: BigNum::from(0),
                },
                regular_deposit
            );
        }

        // Deposit with less than minimum token units
        {
            // Set balance < minimal token units, i.e. `1_000`
            mock_set_balance(
                &token.2,
                leader_account.to_bytes(),
                counterfactual_address.to_bytes(),
                &BigNum::from(999),
            )
            .await
            .expect("Failed to set balance");

            let deposit_with_create2 = eth_adapter
                .get_deposit(&channel, &spender)
                .await
                .expect("should get deposit");

            assert_eq!(
                Deposit {
                    total: BigNum::from(10_000),
                    // tokens are **less** than the minimum tokens required for deposits to count
                    still_on_create2: BigNum::from(0),
                },
                deposit_with_create2
            );
        }

        // Deposit with more than minimum token units
        {
            // Set balance > minimal token units
            mock_set_balance(
                &token.2,
                leader_account.to_bytes(),
                counterfactual_address.to_bytes(),
                &BigNum::from(1_999),
            )
            .await
            .expect("Failed to set balance");

            let deposit_with_create2 = eth_adapter
                .get_deposit(&channel, &spender)
                .await
                .expect("should get deposit");

            assert_eq!(
                Deposit {
                    total: BigNum::from(11_999),
                    // tokens are more than the minimum tokens required for deposits to count
                    still_on_create2: BigNum::from(1_999),
                },
                deposit_with_create2
            );
        }

        // Run sweeper, it should clear the previously set create2 deposit and leave the total
        {
            sweeper_sweep(
                &sweeper.1,
                outpace.0.to_bytes(),
                &channel,
                spender.to_bytes(),
            )
            .await
            .expect("Should sweep the Spender account");

            let swept_deposit = eth_adapter
                .get_deposit(&channel, &spender)
                .await
                .expect("should get deposit");

            assert_eq!(
                Deposit {
                    total: BigNum::from(11_999),
                    // we've just swept the account, so create2 should be empty
                    still_on_create2: BigNum::from(0),
                },
                swept_deposit
            );
        }
    }
}
