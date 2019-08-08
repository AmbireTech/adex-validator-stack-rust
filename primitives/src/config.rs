use std::collections::HashMap;

use crate::BigNum;

#[derive(Debug, Clone)]
pub struct Config {
    pub identity: String,
    pub validators_whitelist: Vec<String>,
    pub creators_whitelist: Vec<String>,
    pub assets_whitelist: Vec<String>,
    pub minimal_deposit: BigNum,
    pub minimal_fee: BigNum,
}