#![cfg_attr(not(feature = "export-abi"), no_main)]

// Core Stylus imports
extern crate alloc;

use alloc::vec::Vec;
use rs_merkle::{
    Hasher, MerkleProof, MerkleTree
};
use tiny_keccak::{Keccak, Hasher as _};
use alloy_primitives::{B256, U256};
use stylus_sdk::{
    stylus_proc::entrypoint,
    prelude::*,
};

// --- 1. Custom Keccak256 Hasher for rs_merkle ---
// rs_merkle requires a type that implements its Hasher trait.
// This implementation uses the no_std compatible tiny_keccak for the hash function.

/// Custom Keccak256 hasher compatible with rs_merkle
pub struct Keccak256Hasher;

impl Hasher for Keccak256Hasher {
    type Hash = [u8; 32];

    fn hash(data: &[u8]) -> [u8; 32] {
        let mut keccak = Keccak::v256();
        keccak.update(data);
        let mut output = [0u8; 32];
        keccak.finalize(&mut output);
        output
    }

    /// Concatenates two hashes (left and right) and hashes the result.
    /// Standard EVM Merkle trees usually concatenate without prefixes.
    fn hash_leaf(leaf: &[u8]) -> [u8; 32] {
        // In many EVM contexts, the leaf is often just the hash of the data.
        // If the leaf is already a hash, you may skip hashing here.
        // For simplicity, we assume we are hashing the leaf data provided.
        Self::hash(leaf)
    }

    fn hash_node(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
        let mut combined = [0u8; 64];
        combined[..32].copy_from_slice(left);
        combined[32..].copy_from_slice(right);
        Self::hash(&combined)
    }
}

// --- 2. Stylus Contract Implementation ---

// Defines the storage structure for our contract
sol_storage! {
    #[entrypoint]
    pub struct MerkleVerifier {
        // The root hash stored on-chain to verify proofs against
        #[borrow]
        pub merkle_root: B256,
    }
}

// Defines a public function callable from other contracts or EOA
#[external]
impl MerkleVerifier {
    /// Sets the Merkle Root for the contract.
    pub fn set_root(&mut self, root: B256) -> Result<(), Vec<u8>> {
        self.merkle_root.set(root);
        Ok(())
    }

    /// Verifies a Merkle proof for a given leaf and index.
    ///
    /// The `leaf` is the original data item (e.g., Keccak256 hash of address+amount).
    /// The `proof` is the array of sibling hashes.
    /// The `leaf_index` is the position of the leaf in the original list.
    pub fn verify(
        &self,
        leaf: B256,
        proof: Vec<B256>,
        leaf_index: U256,
        total_leaves: U256,
    ) -> Result<bool, Vec<u8>> {
        let root = self.merkle_root.get();

        let total_leaves: usize = total_leaves.to::<usize>();
        let leaf_index: usize = leaf_index.to::<usize>();

        // Convert proof from B256 (alloy primitive) to [u8; 32]
        let proof_vec: Vec<[u8; 32]> = proof.into_iter().map(|h| h.to_fixed_bytes()).collect();
        let proof_bytes: &[[u8; 32]] = proof_vec.as_slice();

        // 1. Create a MerkleProof instance
        let merkle_proof = MerkleProof::<Keccak256Hasher>::try_from(proof_bytes).map_err(|_| {
            // Error handling for MerkleProof creation
            alloc::string::String::from("Merkle proof conversion failed").into_bytes()
        })?;

        // 2. Prepare leaf for verification
        let leaf_hash = leaf.to_fixed_bytes();
        let leaves_to_prove = &[[u8; 32]; 1];

        // rs_merkle expects the leaf hash, not the original data, so we use the input `leaf` (B256)
        // directly, assuming it has been pre-hashed off-chain.
        leaves_to_prove[0] = leaf_hash;

        // 3. Verify the proof
        let is_valid = merkle_proof.verify(
            root.to_fixed_bytes(), // The root stored on-chain
            &[leaf_index],         // The index of the leaf
            leaves_to_prove,       // The leaf hash(es) being proven
            total_leaves,          // Total number of leaves in the tree
        );

        Ok(is_valid)
    }
}

// This utility function is only for testing/off-chain simulation if needed
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merkle_tree_construction() {
        let leaves = vec![
            Keccak256Hasher::hash(b"leaf1"),
            Keccak256Hasher::hash(b"leaf2"),
            Keccak256Hasher::hash(b"leaf3"),
            Keccak256Hasher::hash(b"leaf4"),
        ];

        let merkle_tree = MerkleTree::<Keccak256Hasher>::from_leaves(&leaves);
        let root = merkle_tree.root().unwrap();

        // Test proof generation and verification for the first leaf
        let leaf_index = 0;
        let proof = merkle_tree.proof(&[leaf_index]);
        let proof_hashes: Vec<[u8; 32]> = proof.proof_hashes().to_vec();

        let merkle_proof = MerkleProof::<Keccak256Hasher>::try_from(proof_hashes.as_slice()).unwrap();

        assert!(merkle_proof.verify(
            root,
            &[leaf_index],
            &[leaves[leaf_index]],
            leaves.len()
        ));
    }
}
