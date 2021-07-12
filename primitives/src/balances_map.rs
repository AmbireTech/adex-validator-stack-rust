use serde::{Deserialize, Serialize};
use std::{
    collections::{
        btree_map::{Entry, IntoIter, Iter, Values},
        BTreeMap,
    },
    iter::FromIterator,
    ops::Index,
};

use crate::{Address, BigNum, UnifiedNum};

pub type UnifiedMap = Map<Address, UnifiedNum>;
pub type BalancesMap = Map<Address, BigNum>;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(transparent)]
pub struct Map<K: Ord, V>(BTreeMap<K, V>);

impl Map<Address, UnifiedNum> {
    pub fn to_precision(&self, precision: u8) -> BalancesMap {
        self.iter()
            .map(|(address, unified_num)| (*address, unified_num.to_precision(precision)))
            .collect()
    }
}

impl<K: Ord, V> Default for Map<K, V> {
    fn default() -> Self {
        Map(BTreeMap::default())
    }
}

impl<K: Ord, V> Index<&'_ K> for Map<K, V> {
    type Output = V;

    fn index(&self, index: &K) -> &Self::Output {
        self.0.index(index)
    }
}

impl<K: Ord, V> Map<K, V> {
    pub fn iter(&self) -> Iter<'_, K, V> {
        self.0.iter()
    }

    pub fn values(&self) -> Values<'_, K, V> {
        self.0.values()
    }

    pub fn get(&self, key: &K) -> Option<&V> {
        self.0.get(key)
    }

    pub fn contains_key(&self, key: &K) -> bool {
        self.0.contains_key(key)
    }

    pub fn entry(&mut self, key: K) -> Entry<'_, K, V> {
        self.0.entry(key)
    }

    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        self.0.insert(key, value)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl<K: Ord, V> FromIterator<(K, V)> for Map<K, V> {
    fn from_iter<I: IntoIterator<Item = (K, V)>>(iter: I) -> Self {
        // @TODO: Is there better way to do this?
        let btree_map: BTreeMap<K, V> = iter.into_iter().collect();

        Map(btree_map)
    }
}

impl<K: Ord, V> IntoIterator for Map<K, V> {
    type Item = (K, V);
    type IntoIter = IntoIter<K, V>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

#[cfg(test)]
mod test {
    use serde_json::json;

    use super::*;
    use crate::util::tests::prep_db::ADDRESSES;

    #[test]
    fn test_unified_map_de_serialization() {
        let unified_map: UnifiedMap = vec![
            (ADDRESSES["leader"].clone(), UnifiedNum::from(50_u64)),
            (ADDRESSES["follower"].clone(), UnifiedNum::from(100_u64)),
        ]
        .into_iter()
        .collect();

        let actual_json = serde_json::to_value(&unified_map).expect("Should serialize it");
        let expected_json = json!({
            "0xC91763D7F14ac5c5dDfBCD012e0D2A61ab9bDED3":"100",
            "0xce07CbB7e054514D590a0262C93070D838bFBA2e":"50"
        });

        assert_eq!(expected_json, actual_json);

        let balances_map_from_json: UnifiedMap =
            serde_json::from_value(actual_json).expect("Should deserialize it");

        assert_eq!(unified_map, balances_map_from_json);
    }

    #[test]
    fn test_balances_map_de_serialization() {
        let balances_map: BalancesMap = vec![
            (ADDRESSES["leader"].clone(), BigNum::from(50_u64)),
            (ADDRESSES["follower"].clone(), BigNum::from(100_u64)),
        ]
        .into_iter()
        .collect();

        let actual_json = serde_json::to_value(&balances_map).expect("Should serialize it");
        let expected_json = json!({
            "0xC91763D7F14ac5c5dDfBCD012e0D2A61ab9bDED3":"100",
            "0xce07CbB7e054514D590a0262C93070D838bFBA2e":"50"
        });

        assert_eq!(expected_json, actual_json);

        let balances_map_from_json: BalancesMap =
            serde_json::from_value(actual_json).expect("Should deserialize it");

        assert_eq!(balances_map, balances_map_from_json);
    }

    #[test]
    fn test_balances_map_deserialization_with_same_keys() {
        // the first is ETH Checksummed, the second is lowercase!
        let json = json!({
            "0xC91763D7F14ac5c5dDfBCD012e0D2A61ab9bDED3":"100",
            "0xc91763d7f14ac5c5ddfbcd012e0d2a61ab9bded3":"20",
            "0xce07CbB7e054514D590a0262C93070D838bFBA2e":"50"
        });

        let actual_deserialized: BalancesMap =
            serde_json::from_value(json).expect("Should deserialize it");

        let expected_deserialized: BalancesMap = vec![
            (ADDRESSES["leader"].clone(), BigNum::from(50_u64)),
            // only the second should be accepted, as it appears second in the string and it's the latest one
            (ADDRESSES["follower"].clone(), BigNum::from(20_u64)),
        ]
        .into_iter()
        .collect();

        assert_eq!(expected_deserialized, actual_deserialized);
    }
}
