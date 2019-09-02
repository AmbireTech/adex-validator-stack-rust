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
use hex::ToHex;

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

pub type MerkleItem = [u8; 32];

impl Algorithm<MerkleItem> for KeccakAlgorithm {
    #[inline]
    fn hash(&mut self) -> MerkleItem {
        let mut res: [u8; 32] = [0; 32];
        self.0.clone().finalize(&mut res);
        res
    }

    #[inline]
    fn reset(&mut self) {
        self.0 = Keccak::new_sha3_256();
    }
}

pub struct MerkleTree(merkle::MerkleTree<MerkleItem, KeccakAlgorithm, VecStore<MerkleItem>>);

impl MerkleTree {
    pub fn new(leaves: Vec<MerkleItem>) -> MerkleTree {
        let t = merkle::MerkleTree::from_iter(leaves);
        Self(t)
    }

    pub fn root(&self)-> MerkleItem {
        self.0.root()
    }

    pub fn verify(&self, proof: (Vec<MerkleItem>, Vec<bool>) ) -> bool {
        let proof = Proof::new(proof.0, proof.1);
        proof.validate::<KeccakAlgorithm>()
    }

    pub fn proof(&self, i: usize) -> (Vec<MerkleItem>, Vec<bool>) {
        let proof = self.0.gen_proof(i);
        let path = proof.path();
        let lemma = proof.lemma();
        (lemma.to_owned(), path.to_owned())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn it_gets_the_root_of_single_item() {
        let mut h1 = [0u8; 32];
        let mut h2 = [0u8; 32];
        let mut h3 = [0u8; 32];
        h1[0] = 0x11;
        h2[0] = 0x22;
        h3[0] = 0x33;

        let t: MerkleTree = MerkleTree::new(vec![h1, h2, h3]);
        let proof =  t.proof(1);
        let verify = t.verify(proof.clone());
        assert_eq!(verify, true, "should verify proof successfully");
    }
}
