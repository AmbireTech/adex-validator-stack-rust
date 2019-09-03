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

    fn leaf(&mut self, leaf: MerkleItem) -> MerkleItem {
        self.write(leaf.as_ref());
        self.hash()
    }

    fn node(&mut self, left: MerkleItem, right: MerkleItem, _height: usize) -> MerkleItem {
        let mut buf: Vec<u8> = left.iter().cloned().collect();
        let mut buf1: Vec<u8> = right.iter().cloned().collect();
        buf.append(&mut buf1);
        buf.sort();
        println!(" buf len {}", buf.len());

        let mut buf_slice: [u8; 64] = [0; 64];
        buf_slice.copy_from_slice(buf.as_slice());

        self.write(buf_slice.as_ref());
        self.hash()
    }
}

pub struct MerkleTree(merkle::MerkleTree<MerkleItem, KeccakAlgorithm, VecStore<MerkleItem>>);

impl MerkleTree {
    pub fn new(_leaves: Vec<MerkleItem>) -> MerkleTree {
        let mut leaves = _leaves;
        leaves.sort();
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
    fn it_generates_correct_proof() {
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

    #[test]
    fn it_works_okay_with_js_impl() {
        let mut h1 = hex::decode("item").unwrap();
        let mut h2 = hex::decode("item").unwrap();
        println!("len {}", h1.len());
        let mut h1_slice: [u8; 32] = Default::default();
        h1_slice.copy_from_slice(h1.as_slice());

        println!("{:?}", hex::encode(h1_slice));

        let mut h2_slice: [u8; 32] = Default::default();
        h2_slice.copy_from_slice(h2.as_slice());

        println!("{:?}", hex::encode(h1_slice));

        let t: MerkleTree = MerkleTree::new(vec![h1_slice, h2_slice]);
        let root = t.root();
        println!("root {:?}", hex::encode(root));
        // let leaves = t.
        let proof =  t.proof(1);
        let verify = t.verify(proof.clone());
        assert_eq!(verify, true, "should verify proof successfully");
    }
}
