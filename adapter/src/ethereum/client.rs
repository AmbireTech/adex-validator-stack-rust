use std::{fs, str::FromStr};

use crate::{
    prelude::*,
    primitives::{Deposit, Session},
};
use async_trait::async_trait;
use chrono::Utc;
use create2::calc_addr;
use ethstore::{
    ethkey::{verify_address, Message, Signature},
    SafeAccount,
};
use primitives::{Address, BigNum, Chain, ChainId, ChainOf, Channel, Config, ValidatorId};

use super::{
    channel::EthereumChannel,
    error::{Error, EwtSigningError, KeystoreError, VerifyError},
    ewt::{self, Payload},
    to_ethereum_signed, LockedWallet, UnlockedWallet, WalletState, DEPOSITOR_BYTECODE_DECODED,
    ERC20_ABI, IDENTITY_ABI, OUTPACE_ABI, SWEEPER_ABI,
};
use serde_json::Value;
use web3::{
    contract::{Contract, Options as ContractOptions},
    ethabi::{encode, Token},
    transports::Http,
    types::{H160, U256},
    Web3,
};

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

#[derive(Debug, Clone)]
pub struct Options {
    pub keystore_file: String,
    pub keystore_pwd: String,
}

#[derive(Debug, Clone)]
/// Ethereum client implementation for the [`crate::Adapter`].
pub struct Ethereum<S = LockedWallet> {
    address: ValidatorId,
    config: Config,
    pub(crate) state: S,
}

pub(crate) trait ChainTransport {
    fn init_web3(&self) -> web3::Result<Web3<Http>>;
}

impl ChainTransport for Chain {
    fn init_web3(&self) -> web3::Result<Web3<Http>> {
        let transport = Http::new(self.rpc.as_str())?;

        Ok(Web3::new(transport))
    }
}

impl Ethereum<LockedWallet> {
    pub fn init(opts: Options, config: &Config) -> Result<Self, Error> {
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

        Ok(Self {
            address,
            config: config.to_owned(),
            state: LockedWallet::KeyStore {
                keystore: keystore_json,
                password: opts.keystore_pwd.into(),
            },
        })
    }
}
impl<S: WalletState> Ethereum<S> {
    /// Checks if the signer of the `hash` & `signature` has privileges,
    /// by using the Identity contract and the passed identity [`Address`]
    /// See <https://eips.ethereum.org/EIPS/eip-1271>
    ///
    /// **Note:** Signature should be `01` suffixed for Eth Sign for this call.
    pub async fn has_privileges(
        &self,
        chain: &Chain,
        identity: Address,
        hash: [u8; 32],
        signature_with_mode: &[u8],
    ) -> Result<bool, Error> {
        // 0x1626ba7e is in little endian
        let magic_value: u32 = 2126128662;
        // u32::MAX = 4294967295
        let _no_access_value: u32 = 0xffffffff;

        let identity_contract = Contract::from_json(
            chain.init_web3()?.eth(),
            H160(identity.to_bytes()),
            &IDENTITY_ABI,
        )
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
                ContractOptions::default(),
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

impl Unlockable for Ethereum<LockedWallet> {
    type Unlocked = Ethereum<UnlockedWallet>;

    fn unlock(&self) -> Result<Ethereum<UnlockedWallet>, <Self::Unlocked as Locked>::Error> {
        let unlocked_wallet = match &self.state {
            LockedWallet::KeyStore { keystore, password } => {
                let json = serde_json::from_value(keystore.clone())
                    .map_err(KeystoreError::Deserialization)?;

                let wallet = SafeAccount::from_file(json, None, &Some(password.clone()))
                    .map_err(|err| Error::WalletUnlock(err.to_string()))?;

                UnlockedWallet {
                    wallet,
                    password: password.clone(),
                }
            }
            LockedWallet::PrivateKey(_priv_key) => todo!(),
        };

        Ok(Ethereum {
            address: self.address,
            config: self.config.clone(),
            state: unlocked_wallet,
        })
    }
}

#[async_trait]
impl<S: WalletState> Locked for Ethereum<S> {
    type Error = Error;

    fn whoami(&self) -> ValidatorId {
        self.address
    }

    fn verify(
        &self,
        signer: ValidatorId,
        state_root: &str,
        signature: &str,
    ) -> Result<bool, Self::Error> {
        if !signature.starts_with("0x") {
            return Err(VerifyError::SignatureNotPrefixed.into());
        }
        let decoded_signature =
            hex::decode(&signature[2..]).map_err(VerifyError::SignatureDecoding)?;
        let address = ethstore::ethkey::Address::from(*signer.as_bytes());
        let signature = Signature::from_electrum(&decoded_signature);
        let state_root = hex::decode(state_root).map_err(VerifyError::StateRootDecoding)?;
        let message = Message::from(to_ethereum_signed(&state_root));

        let verify_address = verify_address(&address, &signature, &message)
            .map_err(VerifyError::PublicKeyRecovery)?;

        Ok(verify_address)
    }

    /// Creates a `Session` from a provided Token by calling the Contract.
    ///
    /// This methods validates that the [`Payload`]'s [`Chain`] is whitelisted in the configuration.
    ///
    /// Does **not** cache the (`Token`, `Session`) pair.
    async fn session_from_token(&self, token: &str) -> Result<Session, Self::Error> {
        let (verified_token, verified) = ewt::Token::verify(token).map_err(Error::VerifyMessage)?;

        if self.whoami() != verified.payload.id {
            return Err(Error::AuthenticationTokenNotIntendedForUs {
                payload: verified.payload,
                whoami: self.whoami(),
            });
        }

        // Check if Payload chain is whitelisted
        let whitelisted_chain = self
            .config
            .find_chain(verified.payload.chain_id)
            .ok_or(Error::ChainNotWhitelisted(verified.payload.chain_id))?
            .chain
            .clone();

        let sess = match &verified.payload.identity {
            Some(identity) => {
                // the Hash for has_privileges should **not** be an Ethereum Signed Message hash

                if self
                    .has_privileges(
                        &whitelisted_chain,
                        *identity,
                        verified_token.message_hash,
                        &verified_token.signature,
                    )
                    .await?
                {
                    Session {
                        era: verified.payload.era,
                        uid: identity.to_owned(),
                        chain: whitelisted_chain,
                    }
                } else {
                    return Err(Error::InsufficientAuthorizationPrivilege);
                }
            }
            None => Session {
                era: verified.payload.era,
                uid: verified.from,
                chain: whitelisted_chain,
            },
        };

        Ok(sess)
    }

    async fn get_deposit(
        &self,
        channel_context: &ChainOf<Channel>,
        depositor_address: Address,
    ) -> Result<Deposit, Self::Error> {
        let channel = channel_context.context;
        let token = &channel_context.token;
        let chain = &channel_context.chain;

        let web3 = chain.init_web3()?;

        let outpace_contract = Contract::from_json(
            web3.eth(),
            channel_context.chain.outpace.as_bytes().into(),
            &OUTPACE_ABI,
        )
        .map_err(Error::ContractInitialization)?;

        let erc20_contract =
            Contract::from_json(web3.eth(), channel.token.as_bytes().into(), &ERC20_ABI)
                .map_err(Error::ContractInitialization)?;

        let sweeper_contract = Contract::from_json(
            web3.eth(),
            channel_context.chain.sweeper.as_bytes().into(),
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
                ContractOptions::default(),
                None,
            )
            .await
            .map_err(Error::ContractQuerying)?;

        let on_outpace = BigNum::from_str(&on_outpace.to_string()).map_err(Error::BigNumParsing)?;

        let counterfactual_address = get_counterfactual_address(
            sweeper_address,
            &channel,
            outpace_address,
            depositor_address,
        );
        let still_on_create2: U256 = erc20_contract
            .query(
                "balanceOf",
                H160(counterfactual_address.to_bytes()),
                None,
                ContractOptions::default(),
                None,
            )
            .await
            .map_err(Error::ContractQuerying)?;

        let still_on_create2: BigNum = still_on_create2
            .to_string()
            .parse()
            .map_err(Error::BigNumParsing)?;

        // Count the create2 deposit only if it's > minimum token units configured
        let deposit = if still_on_create2 > token.min_token_units_for_deposit {
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

#[async_trait]
impl Unlocked for Ethereum<UnlockedWallet> {
    fn sign(&self, state_root: &str) -> Result<String, Error> {
        let state_root = hex::decode(state_root).map_err(VerifyError::StateRootDecoding)?;
        let message = Message::from(to_ethereum_signed(&state_root));
        let wallet_sign = self
            .state
            .wallet
            .sign(&self.state.password, &message)
            // TODO: This is not entirely true, we do not sign an Ethereum Web Token but Outpace state_root
            .map_err(|err| EwtSigningError::SigningMessage(err.to_string()))?;
        let signature: Signature = wallet_sign.into_electrum().into();

        Ok(format!("0x{}", signature))
    }

    fn get_auth(&self, for_chain: ChainId, intended_for: ValidatorId) -> Result<String, Error> {
        let era = Utc::now().timestamp_millis() as f64 / 60000.0;
        let payload = Payload {
            id: intended_for,
            era: era.floor() as i64,
            identity: None,
            address: self.whoami().to_address(),
            chain_id: for_chain,
        };

        let token = ewt::Token::sign(&self.state.wallet, &self.state.password, payload)
            .map_err(Error::SignMessage)?;

        Ok(token.to_string())
    }
}

#[cfg(test)]
mod test {
    use super::{ewt::ETH_SIGN_SUFFIX, Ethereum};
    use crate::ethereum::{
        client::ChainTransport,
        ewt::{self, Payload},
        get_counterfactual_address,
        test_util::*,
        to_ethereum_signed,
    };

    use crate::{
        prelude::*,
        primitives::{Deposit, Session},
    };
    use chrono::Utc;
    use ethstore::ethkey::Message;

    use primitives::{
        config::GANACHE_CONFIG,
        test_util::{ADDRESS_3, ADDRESS_4, ADDRESS_5, ADVERTISER, CREATOR, LEADER},
        BigNum, ChainOf, ToHex, ValidatorId,
    };
    use web3::{
        contract::Options as ContractOptions, ethabi::Token, signing::keccak256, types::H160,
    };

    #[test]
    fn should_init_and_unlock_ethereum_adapter() {
        let _eth_adapter = Ethereum::init(KEYSTORE_IDENTITY.1.clone(), &GANACHE_CONFIG)
            .expect("Should init")
            .unlock()
            .expect("should unlock eth adapter");
    }

    #[test]
    fn should_get_whoami_sign_and_verify_messages() {
        // whoami
        let eth_adapter = Ethereum::init(KEYSTORE_IDENTITY.1.clone(), &GANACHE_CONFIG)
            .expect("Should init")
            .unlock()
            .expect("should unlock eth adapter");
        let whoami = eth_adapter.whoami();
        assert_eq!(
            whoami.to_string(),
            "0x2bDeAFAE53940669DaA6F519373f686c1f3d3393",
            "failed to get correct whoami"
        );

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

        let signer2 = "0xce07CbB7e054514D590a0262C93070D838bFBA2e"
            .parse()
            .expect("Valid ValidatorId");
        let verify2 = eth_adapter
            .verify(signer2, message2, signature2)
            .expect("Failed to verify signatures");

        assert!(verify, "invalid signature 1 verification");
        assert!(verify2, "invalid signature 2 verification");
    }

    /// Validated using `lib/protocol-eth/js/Bundle.js`
    #[tokio::test]
    async fn test_has_privileges_with_raw_data() {
        let user = *ADDRESS_4;
        // the web3 from Adapter is used to deploy the Identity contract
        let user_adapter = Ethereum::init(KEYSTORES[&user].clone(), &GANACHE_CONFIG)
            .expect("should init ethereum adapter")
            .unlock()
            .expect("should unlock eth adapter");

        let ganache_chain = GANACHE_1337.clone();
        let web3 = ganache_chain
            .init_web3()
            .expect("Should init the Web3 client");

        let evil = *ADDRESS_5;
        let identify_as = *ADDRESS_3;

        let msg_hash_actual = keccak256(&hex::decode("21851b").unwrap());

        let msg_hash = "978d98785935b5526480e9ca00d01b063a6c56f51afbc4ad27b28daecb14258d";
        let msg_hash_decoded = hex::decode(msg_hash).unwrap();
        assert_eq!(msg_hash_decoded.len(), 32);
        assert_eq!(&hex::encode(msg_hash_actual), msg_hash);
        assert_eq!(&msg_hash_decoded, &msg_hash_actual);

        let (identity_address, _contract) = deploy_identity_contract(&web3, identify_as, &[user])
            .await
            .expect("Should deploy identity");

        // User should have privileges!
        {
            let user_sig = "0x41fb9082f8d369256ed7f46afff32efe1511128b92d0be3b2457f012e389320f348700a58395d8f55836cf5d95db431b1061d30b11114d21bd0c5d9dcd4791e71b01";

            let signature_actual = {
                let ethers_sign_message = to_ethereum_signed(&msg_hash_actual);
                let message = Message::from(ethers_sign_message);

                let mut signature = user_adapter
                    .state
                    .wallet
                    .sign(&user_adapter.state.password, &message)
                    .expect("Should sign message")
                    .into_electrum()
                    .to_vec();

                signature.extend(ETH_SIGN_SUFFIX.as_slice());
                signature
            };

            assert_eq!(user_sig, signature_actual.to_hex_prefixed());

            let has_privileges = user_adapter
                .has_privileges(
                    &ganache_chain,
                    identity_address,
                    msg_hash_actual,
                    &signature_actual,
                )
                .await
                .expect("Should get privileges");

            assert!(has_privileges, "User should have privileges!")
        }

        // Evil should **not** have privileges!
        {
            let evil_sig = "0x78b1d955c38baa3d98e11c5ed379191880d9f85acde31a1b7da1436825f688ec1b0a12bc749701a0135643dfb7f84a65697afa8065f886edb259f500651e415b1b01";

            let evil_adapter = Ethereum::init(KEYSTORES[&evil].clone(), &GANACHE_CONFIG)
                .expect("should init ethereum adapter")
                .unlock()
                .expect("should unlock eth adapter");

            let signature_actual = {
                let ethers_sign_message = to_ethereum_signed(&msg_hash_actual);
                let message = Message::from(ethers_sign_message);

                let mut signature = evil_adapter
                    .state
                    .wallet
                    .sign(&evil_adapter.state.password, &message)
                    .expect("Should sign message")
                    .into_electrum()
                    .to_vec();

                signature.extend(ETH_SIGN_SUFFIX.as_slice());
                signature
            };

            assert_eq!(evil_sig, signature_actual.to_hex_prefixed());

            let has_privileges = evil_adapter
                .has_privileges(
                    &ganache_chain,
                    identity_address,
                    msg_hash_actual,
                    &signature_actual,
                )
                .await
                .expect("Should get privileges");

            assert!(!has_privileges, "Evil should not have privileges!")
        }
    }

    #[tokio::test]
    async fn test_has_privileges_with_payload() {
        let adapter = Ethereum::init(KEYSTORES[&LEADER].clone(), &GANACHE_CONFIG)
            .expect("should init ethereum adapter")
            .unlock()
            .expect("should unlock eth adapter");

        let whoami = adapter.whoami().to_address();
        assert_eq!(
            *LEADER, whoami,
            "Ethereum address should be authenticated with keystore file as LEADER!"
        );

        let ganache_chain = GANACHE_1337.clone();
        let web3 = ganache_chain
            .init_web3()
            .expect("Should init the Web3 client");

        let (identity_address, contract) = deploy_identity_contract(&web3, *CREATOR, &[whoami])
            .await
            .expect("Should deploy identity");

        let set_privileges: [u8; 32] = contract
            .query(
                "privileges",
                Token::Address(H160(whoami.to_bytes())),
                None,
                ContractOptions::default(),
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

        let era = Utc::now().timestamp_millis() as f64 / 60000.0;
        let payload = Payload {
            id: adapter.whoami(),
            era: era.floor() as i64,
            address: adapter.whoami().to_address(),
            identity: Some(identity_address),
            chain_id: ganache_chain.chain_id,
        };

        let auth_token = ewt::Token::sign(&adapter.state.wallet, &adapter.state.password, payload)
            .expect("Should sign successfully the Payload");

        let has_privileges = adapter
            .has_privileges(
                &ganache_chain,
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
        let adapter = Ethereum::init(KEYSTORES[&LEADER].clone(), &GANACHE_CONFIG)
            .expect("should init Leader ethereum adapter")
            .unlock()
            .expect("should unlock eth adapter");

        let ganache_chain = GANACHE_1337.clone();
        let web3 = ganache_chain
            .init_web3()
            .expect("Should init the Web3 client");

        let (identity_address, _contract) =
            deploy_identity_contract(&web3, *CREATOR, &[*ADVERTISER])
                .await
                .expect("Should deploy identity");

        let signer_adapter = Ethereum::init(KEYSTORES[&ADVERTISER].clone(), &GANACHE_CONFIG)
            .expect("should init Advertiser ethereum adapter")
            .unlock()
            .expect("Should unlock eth adapter");

        assert_eq!(signer_adapter.whoami(), ValidatorId::from(*ADVERTISER));

        let era = Utc::now().timestamp_millis() as f64 / 60000.0;
        let payload = Payload {
            // the intended ValidatorId for whom the payload is.
            id: adapter.whoami(),
            era: era.floor() as i64,
            // the identity as which we'd like to authenticate
            identity: Some(identity_address),
            // The singer address
            address: signer_adapter.whoami().to_address(),
            // the chain we need to make the token for
            chain_id: ganache_chain.chain_id,
        };

        let token = ewt::Token::sign(
            &signer_adapter.state.wallet,
            &signer_adapter.state.password,
            payload,
        )
        .expect("Should sign successfully the Payload");

        // double check that we have privileges for _Who Am I_
        assert!(adapter
            .has_privileges(
                &ganache_chain,
                identity_address,
                token.message_hash,
                &token.signature
            )
            .await
            .expect("Ok"));

        let session: Session = adapter.session_from_token(token.as_str()).await.unwrap();
        assert_eq!(session.uid, identity_address);
    }

    #[tokio::test]
    async fn get_deposit_and_count_create2_when_min_tokens_received() {
        let web3 = GANACHE_WEB3.clone();

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

        let (config, chain_context) = {
            let mut init_chain = GANACHE_1337.clone();
            init_chain.outpace = outpace.0;
            init_chain.sweeper = sweeper.0;

            let mut config = GANACHE_CONFIG.clone();

            // Assert that the Ganache chain exist in the configuration
            let mut config_chain = config
                .chains
                .values_mut()
                .find(|chain_info| chain_info.chain.chain_id == init_chain.chain_id)
                .expect("Should find Ganache chain in the configuration");

            // override the chain to use the outpace & sweeper addresses that were just deployed
            config_chain.chain = init_chain.clone();

            // Assert that the token that was just deploy does not exist in the Config
            assert!(
                config_chain
                    .tokens
                    .values()
                    .find(|config_token_info| config_token_info.address == token.1)
                    .is_none(),
                "Config should not have this token address, we've just deployed the contract."
            );

            let token_exists = config_chain.tokens.insert("TOKEN".into(), token.0.clone());

            // Assert that the token name that was just deploy does not exist in the Config
            assert!(
                token_exists.is_none(),
                "This token name should not pre-exist in Ganache config"
            );

            let chain_context = ChainOf::new(init_chain, token.0.clone());

            (config, chain_context)
        };

        let channel = get_test_channel(token_address);
        let channel_context = chain_context.with(channel);

        // since we deploy a new contract, it's should be different from all the ones found in config.
        let eth_adapter = Ethereum::init(KEYSTORES[&LEADER].clone(), &config)
            .expect("should init ethereum adapter")
            .unlock()
            .expect("should unlock eth adapter");

        let counterfactual_address =
            get_counterfactual_address(sweeper.0, &channel, outpace.0, spender);

        // No Regular nor Create2 deposits
        {
            let no_deposits = eth_adapter
                .get_deposit(&channel_context, spender)
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

        // 10^18 = 1 TOKEN
        let one_token = {
            let deposit = "1000000000000000000".parse::<BigNum>().unwrap();
            // make sure 1 TOKEN is the minimum set in Config
            let config_token = eth_adapter
                .config
                .find_chain_of(channel.token)
                .expect("Channel token should be present in Config")
                .token;

            assert!(
                deposit >= config_token.min_token_units_for_deposit,
                "The minimum deposit should be >= the configured token minimum token units"
            );

            deposit
        };

        // Regular deposit in Outpace without Create2
        {
            assert!(token.1 == channel.token);
            mock_set_balance(&token.2, LEADER.to_bytes(), spender.to_bytes(), &one_token)
                .await
                .expect("Failed to set balance");

            outpace_deposit(&outpace.1, &channel, spender.to_bytes(), &one_token)
                .await
                .expect("Should deposit funds");

            let regular_deposit = eth_adapter
                .get_deposit(&channel_context, spender)
                .await
                .expect("should get deposit");

            assert_eq!(
                Deposit {
                    total: one_token.clone(),
                    still_on_create2: BigNum::from(0),
                },
                regular_deposit
            );
        }

        // Create2 deposit with less than minimum token units
        // 1 TOKEN = 1 * 10^18
        // 999 * 10^18 < 1 TOKEN
        {
            // Set balance < minimal token units, i.e. 1 TOKEN
            mock_set_balance(
                &token.2,
                leader_account.to_bytes(),
                counterfactual_address.to_bytes(),
                &BigNum::from(999),
            )
            .await
            .expect("Failed to set balance");

            let deposit_with_create2 = eth_adapter
                .get_deposit(&channel_context, spender)
                .await
                .expect("should get deposit");

            assert_eq!(
                Deposit {
                    total: one_token.clone(),
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
                .get_deposit(&channel_context, spender)
                .await
                .expect("should get deposit");

            assert_eq!(
                Deposit {
                    total: &one_token + BigNum::from(1_999),
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
                .get_deposit(&channel_context, spender)
                .await
                .expect("should get deposit");

            assert_eq!(
                Deposit {
                    total: &one_token + BigNum::from(1_999),
                    // we've just swept the account, so create2 should be empty
                    still_on_create2: BigNum::from(0),
                },
                swept_deposit
            );
        }
    }
}
