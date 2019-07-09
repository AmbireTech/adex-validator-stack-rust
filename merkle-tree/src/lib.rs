#![deny(rust_2018_idioms)]
#![deny(clippy::all)]
use algorithm::KeccakAlgorithm;
use merkletree::merkle::{MerkleTree as OriginalMerkleTree, VecStore};

pub type MerkleTree = OriginalMerkleTree<[u8; 32], KeccakAlgorithm, VecStore<[u8; 32]>>;

mod algorithm {
    use merkletree::hash::Algorithm;
    use sha3::{Digest, Keccak256};
    use std::hash::Hasher;

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
