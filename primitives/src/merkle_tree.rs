use merkletree::hash::Algorithm;
use merkletree::merkle;
use merkletree::merkle::VecStore;
use merkletree::proof::Proof;
use std::hash::Hasher;
use std::iter::FromIterator;
use tiny_keccak::Keccak;

#[derive(Clone)]
struct KeccakAlgorithm(Keccak);

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
        let left_vec: Vec<u8> = left.to_vec();
        let right_vec: Vec<u8> = right.to_vec();

        let mut node_vec = vec![left_vec, right_vec];
        node_vec.sort();

        let flatten_node_vec: Vec<u8> = node_vec.into_iter().flatten().collect();

        self.write(&flatten_node_vec.as_slice());
        self.hash()
    }
}

type ExternalMerkleTree =
    merkletree::merkle::MerkleTree<MerkleItem, KeccakAlgorithm, VecStore<MerkleItem>>;

#[derive(Clone)]
enum Tree {
    SingleItem(MerkleItem),
    MerkleTree(ExternalMerkleTree),
}

pub struct MerkleTree {
    tree: Tree,
    root: MerkleItem,
}

impl MerkleTree {
    pub fn new(data: &[MerkleItem]) -> MerkleTree {
        let mut leaves: Vec<MerkleItem> = data.to_owned();;

        let tree: Tree = if leaves.len() == 1 {
            Tree::SingleItem(leaves.first().unwrap().to_owned())
        } else {
            // sort the merkle tree leaves
            leaves.sort();
            // remove duplicates
            leaves.dedup_by(|a, b| a == b);

            let merkletree = merkle::MerkleTree::from_iter(leaves.clone());
            Tree::MerkleTree(merkletree)
        };

        let root: MerkleItem = match &tree {
            Tree::SingleItem(root) => root.to_owned(),
            Tree::MerkleTree(merkletree) => merkletree.root(),
        };

        MerkleTree { tree, root }
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
    fn it_works_okay_with_js_impl() {
        let h1 = <[u8; 32]>::from_hex(
            "71b1b2ad4db89eea341553b718f51f4f0aac03c6a596c4c0e1697f7b9d9da337",
        )
        .unwrap();
        let h2 = <[u8; 32]>::from_hex(
            "778b613574ae22c119efb252f2a56cb05b0d137f8494c0193f4e015c49f43453",
        )
        .unwrap();

        let top = MerkleTree::new(&[h1, h2]);

        let root = hex::encode(&top.root());

        assert_eq!(
            root, "70d6549669561c65fdc687b87743b67e494e1f4be5d19a2955507220e57baaa6",
            "should generate the correct root"
        );

        let proof = top.proof(0);

        let verify = top.verify(proof.clone());
        assert_eq!(verify, true, "should verify proof successfully");
    }

    #[test]
    fn it_works_okay_with_duplicate_leaves_js_impl() {
        let h1 = <[u8; 32]>::from_hex(
            "71b1b2ad4db89eea341553b718f51f4f0aac03c6a596c4c0e1697f7b9d9da337",
        )
        .unwrap();
        let h2 = <[u8; 32]>::from_hex(
            "778b613574ae22c119efb252f2a56cb05b0d137f8494c0193f4e015c49f43453",
        )
        .unwrap();

        // duplicate leaves
        let top = MerkleTree::new(&[h1, h2, h2]);

        let root = hex::encode(&top.root());

        assert_eq!(
            root, "70d6549669561c65fdc687b87743b67e494e1f4be5d19a2955507220e57baaa6",
            "should generate the correct root"
        );

        let proof = top.proof(0);
        let verify = top.verify(proof.clone());

        assert_eq!(verify, true, "should verify proof successfully");
    }
}
