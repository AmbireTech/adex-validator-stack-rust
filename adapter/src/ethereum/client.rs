use std::{fs, str::FromStr};

use crate::{
    prelude::*,
    primitives::{Deposit, Session},
};
use async_trait::async_trait;
use chrono::Utc;
use ethsign::{KeyFile, Signature};
use primitives::{Address, BigNum, Chain, ChainId, ChainOf, Channel, Config, ValidatorId};

use super::{
    error::{Error, EwtSigningError, KeystoreError, VerifyError},
    ewt::{self, Payload},
    to_ethereum_signed, Electrum, LockedWallet, UnlockedWallet, WalletState, IDENTITY_ABI,
    OUTPACE_ABI,
};
use web3::{
    contract::{Contract, Options as ContractOptions},
    ethabi::Token,
    transports::Http,
    types::{H160, U256},
    Web3,
};

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
        let keystore_json: KeyFile =
            serde_json::from_str(&keystore_contents).map_err(KeystoreError::Deserialization)?;

        let address_bytes = keystore_json
            .address
            .clone()
            .ok_or(KeystoreError::AddressMissing)?;

        let address = Address::from_slice(&address_bytes.0).ok_or(KeystoreError::AddressLength)?;

        Ok(Self {
            address: ValidatorId::from(address),
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
                let wallet = keystore.to_secret_key(password)?;

                UnlockedWallet { wallet }
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

        let signature =
            Signature::from_electrum(&decoded_signature).ok_or(VerifyError::SignatureInvalid)?;
        let state_root = hex::decode(state_root).map_err(VerifyError::StateRootDecoding)?;

        let message = to_ethereum_signed(&state_root);

        // recover the public key using the signature and the eth sign message
        let public_key = signature
            .recover(&message)
            .map_err(|ec_err| VerifyError::PublicKeyRecovery(ec_err.to_string()))?;

        Ok(public_key.address() == signer.as_bytes())
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
        let chain = &channel_context.chain;

        let web3 = chain.init_web3()?;

        let outpace_contract = Contract::from_json(
            web3.eth(),
            H160(channel_context.chain.outpace.to_bytes()),
            &OUTPACE_ABI,
        )
        .map_err(Error::ContractInitialization)?;

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

        let deposit = Deposit { total: on_outpace };

        Ok(deposit)
    }
}

#[async_trait]
impl Unlocked for Ethereum<UnlockedWallet> {
    fn sign(&self, state_root: &str) -> Result<String, Error> {
        let state_root = hex::decode(state_root).map_err(VerifyError::StateRootDecoding)?;
        let message = to_ethereum_signed(&state_root);

        let wallet_sign = self
            .state
            .wallet
            .sign(&message)
            // TODO: This is not entirely true, we do not sign an Ethereum Web Token but Outpace state_root
            .map_err(|err| EwtSigningError::SigningMessage(err.to_string()))?;

        Ok(format!("0x{}", hex::encode(wallet_sign.to_electrum())))
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

        let token = ewt::Token::sign(&self.state.wallet, payload).map_err(Error::SignMessage)?;

        Ok(token.to_string())
    }
}

#[cfg(test)]
mod test {
    use super::{ewt::ETH_SIGN_SUFFIX, Ethereum};
    use crate::ethereum::{
        client::ChainTransport,
        ewt::{self, Payload},
        test_util::*,
        to_ethereum_signed, Electrum,
    };

    use crate::{
        prelude::*,
        primitives::{Deposit, Session},
    };
    use chrono::Utc;

    use primitives::{
        channel::Nonce,
        config::GANACHE_CONFIG,
        test_util::{
            ADDRESS_3, ADDRESS_4, ADDRESS_5, ADVERTISER, CREATOR, DUMMY_CAMPAIGN, FOLLOWER,
            GUARDIAN, GUARDIAN_2, IDS, LEADER, LEADER_2,
        },
        BigNum, ChainOf, Channel, ToHex, ValidatorId,
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

                let mut signature = user_adapter
                    .state
                    .wallet
                    .sign(&ethers_sign_message)
                    .expect("Should sign message")
                    .to_electrum()
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

                let mut signature = evil_adapter
                    .state
                    .wallet
                    .sign(&ethers_sign_message)
                    .expect("Should sign message")
                    .to_electrum()
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

        let auth_token = ewt::Token::sign(&adapter.state.wallet, payload)
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

        let token = ewt::Token::sign(&signer_adapter.state.wallet, payload)
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
    async fn multi_chain_deposit_from_config() -> Result<(), Box<dyn std::error::Error>> {
        let config = GANACHE_CONFIG.clone();

        let mut channel_1 = DUMMY_CAMPAIGN.channel;
        let channel_1_token = GANACHE_INFO_1.tokens["Mocked TOKEN 1"].clone();
        channel_1.token = channel_1_token.address;

        let chain_of = ChainOf::new(GANACHE_1.clone(), channel_1_token);

        let web3_1 = chain_of
            .chain
            .init_web3()
            .expect("Should init web3 for Chain #1");

        let leader_adapter = Ethereum::init(KEYSTORES[&LEADER].clone(), &config)
            .expect("should init ethereum adapter")
            .unlock()
            .expect("should unlock eth adapter");

        let advertiser = *ADVERTISER;

        let _actual_deposit = leader_adapter
            .get_deposit(&chain_of.clone().with(channel_1), advertiser)
            .await
            .expect("Should get deposit for Channel in Chain 1");

        let token = Erc20Token::new(&web3_1, chain_of.token.clone());
        let outpace = Outpace::new(&web3_1, chain_of.chain.outpace);

        // OUTPACE deposit Chain #1
        // 10 tokens
        {
            let ten = BigNum::with_precision(10, chain_of.token.precision.into());
            token
                .set_balance(LEADER.to_bytes(), advertiser.to_bytes(), &ten)
                .await
                .expect("Failed to set balance");

            outpace
                .deposit(&channel_1, advertiser.to_bytes(), &ten)
                .await
                .expect("Should deposit funds");

            let regular_deposit = leader_adapter
                .get_deposit(&chain_of.clone().with(channel_1), advertiser)
                .await
                .expect("should get deposit");

            assert_eq!(Deposit { total: ten.clone() }, regular_deposit);
        }

        Ok(())
    }

    #[tokio::test]
    async fn multi_chain_deposit_from_deployed_contracts() -> Result<(), Box<dyn std::error::Error>>
    {
        let chain_of_1337 = ChainOf::new(
            GANACHE_1337.clone(),
            GANACHE_INFO_1337.tokens["Mocked TOKEN 1337"].clone(),
        );

        let mut config = GANACHE_CONFIG.clone();

        let channel_chain_1 = {
            let web3 = GANACHE_1.init_web3().expect("Init web3");

            // deploy contracts
            let token = Erc20Token::deploy(&web3, 1_000)
                .await
                .expect("Correct parameters are passed to the Token constructor.");

            let outpace = Outpace::deploy(&web3)
                .await
                .expect("Correct parameters are passed to the OUTPACE constructor.");

            // Add new "Deployed TOKEN"
            let mut ganache_1 = config
                .chains
                .get_mut("Ganache #1")
                .expect("Should have Ganache #1 already in config");

            assert!(
                !ganache_1
                    .tokens
                    .values()
                    .any(|existing_token| { existing_token.address == token.info.address }),
                "The deployed token address should not have existed previously in the config!"
            );

            // Insert the new token in the config
            assert!(
                ganache_1
                    .tokens
                    .insert("Deployed TOKEN".into(), token.info.clone())
                    .is_none(),
                "Should not have previous value for the Deployed TOKEN"
            );

            // Replace Outpace & Sweeper addresses with the ones just deployed.
            ganache_1.chain.outpace = outpace.address;

            let chain_of_1 = ChainOf::new(ganache_1.chain.clone(), token.info.clone());

            chain_of_1.clone().with(Channel {
                leader: IDS[&LEADER],
                follower: IDS[&FOLLOWER],
                guardian: *GUARDIAN,
                token: chain_of_1.token.address,
                nonce: Nonce::from(1_u32),
            })
        };

        let channel_chain_1337 = chain_of_1337.clone().with(Channel {
            leader: IDS[&LEADER_2],
            follower: IDS[&FOLLOWER],
            guardian: *GUARDIAN_2,
            token: chain_of_1337.token.address,
            nonce: Nonce::from(1337_u32),
        });

        let eth_adapter = Ethereum::init(KEYSTORES[&FOLLOWER].clone(), &config)
            .expect("should init ethereum adapter")
            .unlock()
            .expect("should unlock eth adapter");

        let spender = *ADVERTISER;

        // No Regular deposits
        // Chain #1
        {
            let no_deposits = eth_adapter
                .get_deposit(&channel_chain_1, spender)
                .await
                .expect("should get deposit");

            assert_eq!(
                Deposit {
                    total: BigNum::from(0),
                },
                no_deposits
            );
        }

        // No Regular deposits
        // Chain #1337
        {
            let no_deposits = eth_adapter
                .get_deposit(&channel_chain_1337, spender)
                .await
                .expect("should get deposit");

            assert_eq!(
                Deposit {
                    total: BigNum::from(0),
                },
                no_deposits
            );
        }

        // OUTPACE deposit
        // {
        //     mock_set_balance(&token.2, LEADER.to_bytes(), spender.to_bytes(), &one_token)
        //         .await
        //         .expect("Failed to set balance");

        //     outpace_deposit(&outpace.1, &channel, spender.to_bytes(), &one_token)
        //         .await
        //         .expect("Should deposit funds");

        //     let regular_deposit = eth_adapter
        //         .get_deposit(&channel_context, spender)
        //         .await
        //         .expect("should get deposit");

        //     assert_eq!(
        //         Deposit {
        //             total: one_token.clone(),
        //         },
        //         regular_deposit
        //     );
        // }

        Ok(())
    }
}
