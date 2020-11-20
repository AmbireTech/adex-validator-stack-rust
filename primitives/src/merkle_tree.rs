use merkletree::{hash::Algorithm, merkle, merkle::VecStore, proof::Proof};
use std::fmt;
use std::hash::Hasher;
use std::iter::FromIterator;
use thiserror::Error;
use tiny_keccak::Keccak;

#[derive(Clone)]
struct KeccakAlgorithm(Keccak);

impl fmt::Debug for KeccakAlgorithm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Keccak256 Algorithm")
    }
}

impl KeccakAlgorithm {
    pub fn new() -> KeccakAlgorithm {
        KeccakAlgorithm(Keccak::new_keccak256())
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

type MerkleItem = [u8; 32];

impl Algorithm<MerkleItem> for KeccakAlgorithm {
    #[inline]
    fn hash(&mut self) -> MerkleItem {
        let mut res: [u8; 32] = [0; 32];
        self.0.clone().finalize(&mut res);
        res
    }

    #[inline]
    fn reset(&mut self) {
        self.0 = Keccak::new_keccak256()
    }

    fn leaf(&mut self, leaf: MerkleItem) -> MerkleItem {
        leaf
    }

    fn node(&mut self, left: MerkleItem, right: MerkleItem, _height: usize) -> MerkleItem {
        // This is a check for odd number of leaves items
        // left == right since the right is a duplicate of left
        // return the item unencoded as the JS impl
        if left == right {
            left
        } else {
            let mut node_vec = vec![left.to_vec(), right.to_vec()];
            node_vec.sort();

            let flatten_node_vec: Vec<u8> = node_vec.into_iter().flatten().collect();
            self.write(&flatten_node_vec);
            self.hash()
        }
    }
}

type ExternalMerkleTree =
    merkletree::merkle::MerkleTree<MerkleItem, KeccakAlgorithm, VecStore<MerkleItem>>;

#[derive(Debug, Clone)]
enum Tree {
    SingleItem(MerkleItem),
    MerkleTree(ExternalMerkleTree),
}

#[derive(Debug, Error, Eq, PartialEq)]
pub enum Error {
    #[error("No leaves were provided")]
    ZeroLeaves,
}

#[derive(Debug)]
pub struct MerkleTree {
    tree: Tree,
    root: MerkleItem,
}

impl MerkleTree {
    pub fn new(data: &[MerkleItem]) -> Result<MerkleTree, Error> {
        let mut leaves: Vec<MerkleItem> = data.to_owned();
        // sort the MerkleTree leaves
        leaves.sort_unstable();
        // remove duplicates **before** we check the leaves length
        leaves.dedup_by(|a, b| a == b);

        let tree = match leaves.len() {
            0 => return Err(Error::ZeroLeaves),
            // should never `panic!`, we have a single leaf after all
            1 => Tree::SingleItem(leaves.remove(0)),
            _ => {
                let merkletree = merkle::MerkleTree::from_iter(leaves);

                Tree::MerkleTree(merkletree)
            }
        };

        let root: MerkleItem = match &tree {
            Tree::SingleItem(root) => root.to_owned(),
            Tree::MerkleTree(merkletree) => merkletree.root(),
        };

        Ok(MerkleTree { tree, root })
    }

    pub fn root(&self) -> MerkleItem {
        self.root
    }

    pub fn verify(&self, proof: (Vec<MerkleItem>, Vec<bool>)) -> bool {
        let proof = Proof::new(proof.0, proof.1);
        proof.validate::<KeccakAlgorithm>()
    }

    pub fn proof(&self, i: usize) -> (Vec<MerkleItem>, Vec<bool>) {
        match &self.tree {
            Tree::SingleItem(_) => (vec![], vec![]),
            Tree::MerkleTree(merkle) => {
                let proof = merkle.gen_proof(i);
                let path = proof.path();
                let lemma = proof.lemma();
                (lemma.to_owned(), path.to_owned())
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use hex::FromHex;

    #[test]
    fn it_returns_error_on_zero_leaves() {
        let error = MerkleTree::new(&[]).expect_err("ZeroLeaves error expected");
        assert_eq!(Error::ZeroLeaves, error);
    }

    #[test]
    fn it_generates_correct_merkle_tree_that_correlates_with_js_impl() {
        let h1 = <[u8; 32]>::from_hex(
            "71b1b2ad4db89eea341553b718f51f4f0aac03c6a596c4c0e1697f7b9d9da337",
        )
        .unwrap();
        let h2 = <[u8; 32]>::from_hex(
            "778b613574ae22c119efb252f2a56cb05b0d137f8494c0193f4e015c49f43453",
        )
        .unwrap();

        let top = MerkleTree::new(&[h1, h2]).expect("Should create MerkleTree");

        let root = hex::encode(&top.root());

        assert_eq!(
            root, "70d6549669561c65fdc687b87743b67e494e1f4be5d19a2955507220e57baaa6",
            "should generate the correct root"
        );

        let proof = top.proof(0);

        let verify = top.verify(proof);
        assert_eq!(verify, true, "should verify proof successfully");
    }

    #[test]
    fn it_generates_correct_merkle_tree_with_duplicate_leaves() {
        let h1 = <[u8; 32]>::from_hex(
            "71b1b2ad4db89eea341553b718f51f4f0aac03c6a596c4c0e1697f7b9d9da337",
        )
        .unwrap();
        let h2 = <[u8; 32]>::from_hex(
            "778b613574ae22c119efb252f2a56cb05b0d137f8494c0193f4e015c49f43453",
        )
        .unwrap();

        // duplicate leaves
        let top = MerkleTree::new(&[h1, h2, h2]).expect("Should create MerkleTree");

        let root = hex::encode(&top.root());

        assert_eq!(
            root, "70d6549669561c65fdc687b87743b67e494e1f4be5d19a2955507220e57baaa6",
            "should generate the correct root"
        );

        let proof = top.proof(0);
        let verify = top.verify(proof);

        assert_eq!(verify, true, "should verify proof successfully");
    }

    #[test]
    fn it_generates_correct_merkle_tree_with_odd_leaves() {
        let h1 = <[u8; 32]>::from_hex(
            "13c21db99584c9bb3e9ad98061f6ca39364049b328b74822be6303a4da18014d",
        )
        .unwrap();
        let h2 = <[u8; 32]>::from_hex(
            "b1bea7b8b58cd47d475bfe07dbe6df33f50f7a76957c51cebe8254257542fd7d",
        )
        .unwrap();

        let h3 = <[u8; 32]>::from_hex(
            "c455ef23d4db0091e1e25ef5d652a2832a1fc4fa82b8e66c290a692836e0cbe6",
        )
        .unwrap();

        // odd leaves
        let top = MerkleTree::new(&[h1, h2, h3]).expect("Should create MerkleTree");

        let root = hex::encode(&top.root());

        assert_eq!(
            root, "e68ea33571084e5dea276b089a10fa7be9d59accf3d7838c0d9b050bf72634a1",
            "should generate the correct root"
        );

        let proof = top.proof(0);
        let verify = top.verify(proof);

        assert_eq!(verify, true, "should verify proof successfully");
    }
}
