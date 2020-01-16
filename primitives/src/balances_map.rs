use std::collections::BTreeMap;

use crate::BigNum;
use std::collections::btree_map::{Entry, Iter, Values};

use serde::{ser::SerializeMap, Deserialize, Serialize, Serializer};
use std::iter::FromIterator;

type BalancesKey = String;
type BalancesValue = BigNum;

#[serde(transparent)]
#[derive(Clone, Debug, Deserialize, Default)]
pub struct BalancesMap(
    #[serde(serialize_with = "serialize_balances_map")] BTreeMap<BalancesKey, BalancesValue>,
);

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

impl FromIterator<(BalancesKey, BalancesValue)> for BalancesMap {
    fn from_iter<I: IntoIterator<Item = (BalancesKey, BalancesValue)>>(iter: I) -> Self {
        // @TODO: Is there better way to do this?
        let btree_map: BTreeMap<BalancesKey, BalancesValue> = iter.into_iter().collect();

        BalancesMap(btree_map)
    }
}

impl Serialize for BalancesMap {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (key, big_num) in self.0.iter() {
            map.serialize_entry(&key.to_lowercase(), big_num)?;
        }
        map.end()
    }
}
