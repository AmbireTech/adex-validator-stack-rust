use hex::encode;

//use crate::adapter::{
//    Adapter, AdapterError, AdapterFuture, BalanceRoot, ChannelId, Config, SignableStateRoot,
//};
//
use std::collections::HashMap;
use std::fmt;
use std::fs;

use futures::future::{err, ok, FutureExt};
use serde::{Deserialize, Serialize};

use primitives::channel_validator::{ChannelValidator};
use primitives::adapter::{Adapter, AdapterFuture, EthereumAdapterOptions};
use primitives::config::{Config};
//
//use domain::validator::message::State;
//
//#[derive(Debug)]
//pub struct DummyParticipant {
//    pub identity: String,
//    pub token: String,
//}

pub struct EthereumAdapter {
    keystoreJson: String,
    keystorePwd: String,
    authTokens: HashMap<String, String>,
    verifiedAuth:  HashMap<String, String>,
//    wallet:
//    provider:
}

// Enables DummyAdapter to be able to
// check if a channel is valid
impl ChannelValidator for DummyAdapter {}

impl Adapter for DummyAdapter {
    fn init(self, opts: &EthereumAdapterOptions, config: &Config) -> Self {
        // check if file exists
        let contents = fs::read_to_string(opts.keystoreFile)
            .expect("keystoreFile required");


    }

    fn unlock() -> Self {

    }
}