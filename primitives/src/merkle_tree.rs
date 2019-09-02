use std::fmt;
use std::hash::Hasher;
use std::iter::FromIterator;
use crypto::sha3::{Sha3, Sha3Mode};
use crypto::digest::Digest;
use merkletree::hash::{Algorithm, Hashable};
use merkletree::merkle;
use merkletree::merkle::{VecStore};
use merkletree::proof::Proof;
use tiny_keccak::Keccak;

#[derive(Clone)]
struct KeccakAlgorithm(Keccak);

impl KeccakAlgorithm {
    pub fn new() -> KeccakAlgorithm {
        KeccakAlgorithm(Keccak::new_sha3_256())
    }
}

impl Default for KeccakAlgorithm {
    fn default() -> KeccakAlgorithm {
        KeccakAlgorithm::new()
    }
}

impl Hasher for KeccakAlgorithm {
    #[inline]
    fn write(&mut self, msg: &[u8]) {
        self.0.update(msg)
    }

    #[inline]
    fn finish(&self) -> u64 {
        unimplemented!()
    }
}

impl Algorithm<[u8; 32]> for KeccakAlgorithm {
    #[inline]
    fn hash(&mut self) -> [u8; 32] {
        let mut res: [u8; 32] = [0; 32];
        self.0.clone().finalize(&mut res);
        res
    }

    #[inline]
    fn reset(&mut self) {
        self.0 = Keccak::new_sha3_256();
    }
}

pub struct MerkleTree(merkle::MerkleTree<[u8; 32], KeccakAlgorithm, VecStore<[u8; 32]>>);

impl MerkleTree {
    pub fn new(leaves: Vec<[u8; 32]>) -> MerkleTree {
        let t = merkle::MerkleTree::from_iter(leaves);
        Self(t)
    }

    pub fn root(&self)-> [u8; 32] {
        self.0.root()
    }

    pub fn verify(&self, proof: (Vec<[u8; 32]>, Vec<bool>)) -> bool {
        let proof = Proof::new(proof.0, proof.1);
        proof.validate::<KeccakAlgorithm>()
    }

    pub fn proof(&self, i: usize) -> (Vec<bool>, Vec<[u8; 32]>) {
        let proof = self.0.gen_proof(i);
        let path = proof.path();
        let lemma = proof.lemma();
        (path.to_owned(), lemma.to_owned())
    }
}