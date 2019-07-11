#![deny(rust_2018_idioms)]
#![deny(clippy::all)]

use merkletree::hash::{Algorithm, Hashable};
use merkletree::merkle::VecStore;

use algorithm::KeccakAlgorithm;

type Element = [u8; 32];
pub type ExternalMerkleTree =
    merkletree::merkle::MerkleTree<Element, KeccakAlgorithm, VecStore<Element>>;

enum Tree {
    SingleItem(Element),
    MerkleTree(ExternalMerkleTree),
}

pub struct MerkleTree {
    _tree: Tree,
    root: Element,
}

impl MerkleTree {
    pub fn new<I: IntoIterator<Item = Element>>(data: I) -> Self {
        let iter = data.into_iter();
        let leafs = iter.size_hint().1.unwrap();
        assert!(leafs > 0);

        let (tree, root) = if leafs == 1 {
            let elements: Vec<[u8; 32]> = iter.collect();
            let mut algorithm = KeccakAlgorithm::default();

            algorithm.reset();
            elements[0].hash(&mut algorithm);
            let root = algorithm.hash();

            (Tree::SingleItem(elements[0]), root)
        } else {
            let merkle_tree = ExternalMerkleTree::from_data(iter);
            let root = merkle_tree.root();
            (Tree::MerkleTree(merkle_tree), root)
        };

        Self { _tree: tree, root }
    }

    pub fn root(&self) -> Element {
        self.root
    }
}

mod algorithm {
    use std::hash::Hasher;

    use merkletree::hash::Algorithm;
    use sha3::{Digest, Keccak256};

    pub struct KeccakAlgorithm(Keccak256);

    impl KeccakAlgorithm {
        pub fn new() -> KeccakAlgorithm {
            KeccakAlgorithm(Keccak256::new())
        }
    }

    impl Default for KeccakAlgorithm {
        fn default() -> KeccakAlgorithm {
            KeccakAlgorithm::new()
        }
    }

    impl Hasher for KeccakAlgorithm {
        #[inline]
        fn finish(&self) -> u64 {
            unimplemented!()
        }

        #[inline]
        fn write(&mut self, msg: &[u8]) {
            self.0.input(msg)
        }
    }

    impl Algorithm<[u8; 32]> for KeccakAlgorithm {
        #[inline]
        fn hash(&mut self) -> [u8; 32] {
            let mut h = [0u8; 32];
            let hash_result = self.0.clone().result();
            h.copy_from_slice(&hash_result[..32]);
            h
        }

        #[inline]
        fn reset(&mut self) {
            self.0.reset();
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn it_hashes_single_item() {
        let item = b"01234567890123456789012345678901";
        // the hash of the item
        let hash = "b6f6f0bb62127422171f029ef8588af3a45d58989134675112c2acc78dd16078";

        let merkle_tree = MerkleTree::new(vec![*item]);

        assert_eq!(hash, hex::encode(merkle_tree.root()));
    }
}
