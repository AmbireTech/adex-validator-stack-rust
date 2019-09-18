#![deny(clippy::all)]
#![deny(rust_2018_idioms)]

use futures::future::{ok, FutureExt};
use primitives::adapter::{Adapter, AdapterOptions, AdapterResult};
use primitives::channel_validator::ChannelValidator;
use primitives::config::Config;
use primitives::{Channel, ValidatorDesc};
use std::collections::HashMap;
use std::fs::File;
use web3::futures::Future;
use web3::types::{Address};
use web3::contract::{Contract, Options};
use crate::EthereumChannel;
use std::path::{Path, PathBuf};
use ethsign::{
    keyfile::{Bytes, KeyFile},
    Protected,
};
// use ethsign::{PublicKey, SecretKey, Signature};
use ethkey::{Signature, KeyPair};

use std::error::Error;

pub type Password = Protected;
// pub type Message = [u8; 32];

// #[derive(Debug, Clone)]
// pub struct EthAccount {
//     pub secret: SecretKey,
//     pub public: PublicKey,
//     pub address: Address,
// }

// impl EthAccount {
//     pub fn sign(&self, msg: &Message) -> Result<Signature, Box<dyn Error>> {
//         Ok(self.secret.sign(msg)?)
//     }

//     /// verifies signature for given message and self public key
//     pub fn verify(&self, sig: &Signature, msg: &Message) -> Result<bool, Box<dyn Error>> {
//         Ok(self.public.verify(sig, msg)?)
//     }
// }

#[derive(Debug, Clone)]
pub struct EthereumAdapter {
    keystore_json: String,
    keystore_pwd: String,
    ethereum_core_address: String, 
    ethereum_network: String,
    auth_tokens: HashMap<String, String>,
    verified_auth: HashMap<String, String>,
    wallet: Option<KeyPair>
}

// Enables EthereumAdapter to be able to
// check if a channel is valid
impl ChannelValidator for EthereumAdapter {}

// @TODO
impl Adapter for EthereumAdapter {
    type Output = EthereumAdapter;

    fn init(opts: AdapterOptions, config: &Config) -> EthereumAdapter {
        // @TODO ensure the keystore_json file exists
        // during program startup
        let keystore_json = opts.keystore_file.expect("Keystore file required");
        let keystore_pwd = opts.keystore_pwd.expect("Keystore password required");

        Self {
            keystore_json,
            keystore_pwd,
            auth_tokens: HashMap::new(),
            verified_auth: HashMap::new(),
            wallet: None,
            ethereum_network: config.ethereum_network,
            ethereum_core_address: config.ethereum_core_address
        }
    }

    fn unlock(&self) -> AdapterResult<bool> {
        let path = Path::new(&self.keystore_json).to_path_buf();
        let password: Password = self.keystore_pwd.into();

        let json_file = File::open(&path).expect("Failed to load json file");
        let key_file: KeyFile = serde_json::from_reader(json_file).expect("Invalid keystore json");
        // let secret = key_file.to_secret_key(&password).expect("Invalid Keystore password");
        let plain_secret = key_file.crypto.decrypt(&password).expect("Invalid keystore password");

        let keypair = KeyPair::from_secret_slice(&plain_secret.as_slice()).expect("Failed to create keypair");

        self.wallet = Some(keypair);
    
        // wallet has been unlocked
        Ok(true)
    }

    fn whoami(&self) -> String {
        match self.wallet {
            Some(wallet) =>  format!("0x{}", wallet.address()),
            None => {
                eprintln!("Unlock wallet before use");
                "".to_string()
            }
        }
    }

    fn sign(&self, state_root: &[u8; 32]) -> AdapterResult<String> {
        let wallet = self.wallet.expect("Unlock the wallet before signing");
        let signature = wallet.sign(state_root).expect("sign message");
        
        format!("{}", signature)
    }

    fn verify(&self, _signer: &str, state_root: &[u8; 32], signature: &[u8; 32]) -> AdapterResult<bool> {

        self.wallet.verify(signature, state_root)
    }

    fn validate_channel(&self, channel: &Channel) -> AdapterResult<bool> {
        let contract = Contract::from_json(
            self.web3.eth(), 
            self.ethereum_core_address.into(), 
            include_bytes!("../contract/AdExCore.json"),
        ).unwrap();

        let eth_channel: EthereumChannel = EthereumChannel::new();

        // assert_eq!(channel.id, )

        


    }

    fn session_from_token(&self, _token: &str) -> AdapterResult<String> {
        // @TODO
        Ok("hello".to_string())
    }

    fn get_auth(&self, _validator: &ValidatorDesc) -> AdapterResult<String> {
        // let participant = self
        //     .participants
        //     .iter()
        //     .find(|&(_, participant)| participant.identity == validator);
        // let future = match participant {
        //     Some((_, participant)) => ok(participant.token.to_string()),
        //     None => err(AdapterError::Authentication(
        //         "Identity not found".to_string(),
        //     )),
        // };
        Ok("auth".to_string())
    }
}
