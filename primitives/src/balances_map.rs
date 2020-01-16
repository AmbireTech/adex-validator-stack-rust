use std::collections::BTreeMap;

use crate::BigNum;
use serde_hex::{SerHexSeq, StrictPfx};
use std::collections::btree_map::{Entry, Iter, Values};

use serde::{Deserialize, Serialize};
use std::iter::FromIterator;
use std::ops::Deref;

#[derive(Debug, Clone, Serialize, Deserialize, PartialOrd, Ord, PartialEq, Eq)]
#[serde(transparent)]
pub struct BalancesKey(#[serde(with = "SerHexSeq::<StrictPfx>")] Vec<u8>);
impl Deref for BalancesKey {
    type Target = Vec<u8>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

type BalancesValue = BigNum;

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(transparent)]
pub struct BalancesMap(BTreeMap<BalancesKey, BalancesValue>);

impl BalancesMap {
    pub fn iter(&self) -> Iter<'_, BalancesKey, BalancesValue> {
        self.0.iter()
    }

    pub fn values(&self) -> Values<'_, BalancesKey, BalancesValue> {
        self.0.values()
    }

    pub fn get(&self, key: &BalancesKey) -> Option<&BalancesValue> {
        self.0.get(key)
    }

    pub fn entry(&mut self, key: BalancesKey) -> Entry<'_, BalancesKey, BalancesValue> {
        self.0.entry(key)
    }

    pub fn insert(&mut self, key: BalancesKey, value: BalancesValue) -> Option<BalancesValue> {
        self.0.insert(key, value)
    }
}

impl<K: AsRef<[u8]>> FromIterator<(K, BalancesValue)> for BalancesMap {
    fn from_iter<I: IntoIterator<Item = (K, BalancesValue)>>(iter: I) -> Self {
        // @TODO: Is there better way to do this?
        let btree_map: BTreeMap<BalancesKey, BalancesValue> = iter
            .into_iter()
            .map(|(k, v)| (BalancesKey(k.as_ref().to_vec()), v))
            .collect();

        BalancesMap(btree_map)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::BigNum;

    #[test]
    fn test_map() {
        let data = vec![
            (
                "0xce07CbB7e054514D590a0262C93070D838bFBA2e".to_string(),
                BigNum::from(50_u64),
            ),
            (
                "0x061d5e2a67d0a9a10f1c732bca12a676d83f79663a396f7d87b3e30b9b411088".to_string(),
                BigNum::from(100_u64),
            ),
        ];

        let balances_map: BalancesMap = data.into_iter().collect();

        let actual_json = serde_json::to_string(&balances_map).expect("Should serialize it");

        let balances_map_from_json: BalancesMap =
            serde_json::from_str(&string).expect("Should deserialize it");

        assert_eq!(balances_map, balances_map_from_json);
    }
}
