//! Merkle tree operations for settlement verification (§10.4).
//!
//! This module implements merkle tree construction and proof verification
//! for settlement batches, allowing recipients to verify their inclusion.

use nodalync_crypto::Hash;
use nodalync_types::SettlementEntry;
use sha2::{Digest, Sha256};

use crate::error::{EconError, EconResult};

/// Domain separator for merkle tree hashing
const DOMAIN_MERKLE_LEAF: u8 = 0x01;
const DOMAIN_MERKLE_NODE: u8 = 0x02;

/// Hash a settlement entry for use as a merkle leaf.
///
/// Uses domain separation to distinguish leaf hashes from internal nodes.
///
/// # Arguments
/// * `entry` - The settlement entry to hash
///
/// # Returns
/// The hash of the entry
pub fn hash_settlement_entry(entry: &SettlementEntry) -> Hash {
    let mut hasher = Sha256::new();

    // Domain separator for leaf
    hasher.update([DOMAIN_MERKLE_LEAF]);

    // Hash recipient
    hasher.update(entry.recipient.0);

    // Hash amount as big-endian u64
    hasher.update(entry.amount.to_be_bytes());

    // Hash provenance hashes count and contents
    hasher.update((entry.provenance_hashes.len() as u32).to_be_bytes());
    for hash in &entry.provenance_hashes {
        hasher.update(hash.0);
    }

    // Hash payment ids count and contents
    hasher.update((entry.payment_ids.len() as u32).to_be_bytes());
    for id in &entry.payment_ids {
        hasher.update(id.0);
    }

    let result: [u8; 32] = hasher.finalize().into();
    Hash(result)
}

/// Hash two nodes together for merkle tree construction.
///
/// Orders the hashes lexicographically before combining to ensure
/// deterministic results regardless of input order.
///
/// # Arguments
/// * `a` - First hash
/// * `b` - Second hash
///
/// # Returns
/// The combined hash
fn hash_pair(a: &Hash, b: &Hash) -> Hash {
    let mut hasher = Sha256::new();

    // Domain separator for internal node
    hasher.update([DOMAIN_MERKLE_NODE]);

    // Sort hashes lexicographically for determinism
    if a.0 <= b.0 {
        hasher.update(a.0);
        hasher.update(b.0);
    } else {
        hasher.update(b.0);
        hasher.update(a.0);
    }

    let result: [u8; 32] = hasher.finalize().into();
    Hash(result)
}

/// Compute the merkle root from settlement entries.
///
/// # Arguments
/// * `entries` - The settlement entries to include in the tree
///
/// # Returns
/// The merkle root hash, or a zero hash for empty entries
pub fn compute_merkle_root(entries: &[SettlementEntry]) -> Hash {
    if entries.is_empty() {
        return Hash([0u8; 32]);
    }

    // Compute leaf hashes
    let mut hashes: Vec<Hash> = entries.iter().map(hash_settlement_entry).collect();

    // Build tree bottom-up
    while hashes.len() > 1 {
        let mut next_level = Vec::new();

        for chunk in hashes.chunks(2) {
            if chunk.len() == 2 {
                next_level.push(hash_pair(&chunk[0], &chunk[1]));
            } else {
                // Odd element gets promoted directly
                next_level.push(chunk[0]);
            }
        }

        hashes = next_level;
    }

    hashes.pop().unwrap()
}

/// Compute the batch ID from settlement entries.
///
/// The batch ID is computed by hashing all entry hashes together.
///
/// # Arguments
/// * `entries` - The settlement entries
///
/// # Returns
/// The batch ID hash
pub fn compute_batch_id(entries: &[SettlementEntry]) -> Hash {
    let mut hasher = Sha256::new();

    // Hash all entry hashes together
    for entry in entries {
        let entry_hash = hash_settlement_entry(entry);
        hasher.update(entry_hash.0);
    }

    let result: [u8; 32] = hasher.finalize().into();
    Hash(result)
}

/// A merkle proof for an entry in a settlement batch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MerkleProof {
    /// Sibling hashes along the path to the root
    pub siblings: Vec<Hash>,
    /// Path direction: true = right, false = left (position of the sibling)
    pub path: Vec<bool>,
}

impl MerkleProof {
    /// Create a new merkle proof.
    pub fn new(siblings: Vec<Hash>, path: Vec<bool>) -> Self {
        Self { siblings, path }
    }

    /// Get the depth of the proof (number of levels).
    pub fn depth(&self) -> usize {
        self.siblings.len()
    }
}

/// Create a merkle proof for an entry at a given index.
///
/// # Arguments
/// * `entries` - All settlement entries in the batch
/// * `index` - Index of the entry to prove
///
/// # Returns
/// A merkle proof that can verify the entry's inclusion
///
/// # Errors
/// * `EconError::EmptyEntries` if entries is empty
/// * `EconError::IndexOutOfBounds` if index >= entries.len()
pub fn create_merkle_proof(entries: &[SettlementEntry], index: usize) -> EconResult<MerkleProof> {
    if entries.is_empty() {
        return Err(EconError::EmptyEntries);
    }

    if index >= entries.len() {
        return Err(EconError::IndexOutOfBounds {
            index,
            len: entries.len(),
        });
    }

    // Single entry - empty proof
    if entries.len() == 1 {
        return Ok(MerkleProof::new(Vec::new(), Vec::new()));
    }

    // Compute leaf hashes
    let mut hashes: Vec<Hash> = entries.iter().map(hash_settlement_entry).collect();

    let mut siblings = Vec::new();
    let mut path = Vec::new();
    let mut current_index = index;

    // Build proof by traversing up the tree
    while hashes.len() > 1 {
        let is_right_sibling = current_index % 2 == 0;
        let sibling_index = if is_right_sibling {
            current_index + 1
        } else {
            current_index - 1
        };

        // Get sibling if it exists
        if sibling_index < hashes.len() {
            siblings.push(hashes[sibling_index]);
            path.push(is_right_sibling); // true if sibling is to the right
        }

        // Build next level
        let mut next_level = Vec::new();
        for chunk in hashes.chunks(2) {
            if chunk.len() == 2 {
                next_level.push(hash_pair(&chunk[0], &chunk[1]));
            } else {
                next_level.push(chunk[0]);
            }
        }

        hashes = next_level;
        current_index /= 2;
    }

    Ok(MerkleProof::new(siblings, path))
}

/// Verify a merkle proof for a settlement entry.
///
/// # Arguments
/// * `root` - The expected merkle root
/// * `entry` - The settlement entry being verified
/// * `proof` - The merkle proof
///
/// # Returns
/// `true` if the proof is valid, `false` otherwise
pub fn verify_merkle_proof(root: &Hash, entry: &SettlementEntry, proof: &MerkleProof) -> bool {
    // Empty proof is only valid for single-entry tree
    if proof.siblings.is_empty() && proof.path.is_empty() {
        let entry_hash = hash_settlement_entry(entry);
        return entry_hash == *root;
    }

    if proof.siblings.len() != proof.path.len() {
        return false;
    }

    let mut current_hash = hash_settlement_entry(entry);

    for (sibling, _is_right) in proof.siblings.iter().zip(proof.path.iter()) {
        // hash_pair sorts hashes internally for determinism, so path direction
        // is tracked for reconstruction but not needed for verification
        current_hash = hash_pair(&current_hash, sibling);
    }

    current_hash == *root
}

#[cfg(test)]
mod tests {
    use super::*;
    use nodalync_crypto::{content_hash, generate_identity, peer_id_from_public_key, PeerId};

    fn test_peer_id() -> PeerId {
        let (_, public_key) = generate_identity();
        peer_id_from_public_key(&public_key)
    }

    fn test_hash(data: &[u8]) -> Hash {
        content_hash(data)
    }

    fn test_entry(amount: u64) -> SettlementEntry {
        SettlementEntry::new(
            test_peer_id(),
            amount,
            vec![test_hash(b"prov")],
            vec![test_hash(b"pay")],
        )
    }

    #[test]
    fn test_hash_settlement_entry() {
        let entry1 = test_entry(100);
        let entry2 = test_entry(200);

        let hash1 = hash_settlement_entry(&entry1);
        let hash2 = hash_settlement_entry(&entry2);

        // Different entries should have different hashes
        assert_ne!(hash1, hash2);

        // Same entry should produce same hash
        assert_eq!(hash_settlement_entry(&entry1), hash1);
    }

    #[test]
    fn test_merkle_root_empty() {
        let root = compute_merkle_root(&[]);
        assert_eq!(root, Hash([0u8; 32]));
    }

    #[test]
    fn test_merkle_root_single() {
        let entry = test_entry(100);
        let root = compute_merkle_root(&[entry.clone()]);

        // Single entry: root equals entry hash
        assert_eq!(root, hash_settlement_entry(&entry));
    }

    #[test]
    fn test_merkle_root_two() {
        let entry1 = test_entry(100);
        let entry2 = test_entry(200);

        let root = compute_merkle_root(&[entry1.clone(), entry2.clone()]);

        // Root should be hash of the two leaves combined
        let hash1 = hash_settlement_entry(&entry1);
        let hash2 = hash_settlement_entry(&entry2);
        let expected = hash_pair(&hash1, &hash2);

        assert_eq!(root, expected);
    }

    #[test]
    fn test_merkle_root_deterministic() {
        let entry1 = test_entry(100);
        let entry2 = test_entry(200);
        let entry3 = test_entry(300);

        let root1 = compute_merkle_root(&[entry1.clone(), entry2.clone(), entry3.clone()]);
        let root2 = compute_merkle_root(&[entry1, entry2, entry3]);

        assert_eq!(root1, root2);
    }

    #[test]
    fn test_create_merkle_proof_single() {
        let entry = test_entry(100);
        let proof = create_merkle_proof(&[entry], 0).unwrap();

        // Single entry proof should be empty
        assert!(proof.siblings.is_empty());
        assert!(proof.path.is_empty());
    }

    #[test]
    fn test_create_merkle_proof_index_out_of_bounds() {
        let entry = test_entry(100);
        let result = create_merkle_proof(&[entry], 1);

        assert!(matches!(result, Err(EconError::IndexOutOfBounds { .. })));
    }

    #[test]
    fn test_create_merkle_proof_empty() {
        let result = create_merkle_proof(&[], 0);
        assert!(matches!(result, Err(EconError::EmptyEntries)));
    }

    #[test]
    fn test_verify_merkle_proof_single() {
        let entry = test_entry(100);
        let root = compute_merkle_root(&[entry.clone()]);
        let proof = create_merkle_proof(&[entry.clone()], 0).unwrap();

        assert!(verify_merkle_proof(&root, &entry, &proof));
    }

    #[test]
    fn test_verify_merkle_proof_two() {
        let entry1 = test_entry(100);
        let entry2 = test_entry(200);
        let entries = [entry1.clone(), entry2.clone()];

        let root = compute_merkle_root(&entries);

        // Verify first entry
        let proof1 = create_merkle_proof(&entries, 0).unwrap();
        assert!(verify_merkle_proof(&root, &entry1, &proof1));

        // Verify second entry
        let proof2 = create_merkle_proof(&entries, 1).unwrap();
        assert!(verify_merkle_proof(&root, &entry2, &proof2));
    }

    #[test]
    fn test_verify_merkle_proof_four() {
        let entries: Vec<SettlementEntry> = (0..4).map(|i| test_entry(100 * (i + 1))).collect();

        let root = compute_merkle_root(&entries);

        // Verify all entries
        for (i, entry) in entries.iter().enumerate() {
            let proof = create_merkle_proof(&entries, i).unwrap();
            assert!(
                verify_merkle_proof(&root, entry, &proof),
                "Failed to verify entry {}",
                i
            );
        }
    }

    #[test]
    fn test_verify_merkle_proof_invalid() {
        let entry1 = test_entry(100);
        let entry2 = test_entry(200);
        let entry_wrong = test_entry(999);

        let entries = [entry1.clone(), entry2];
        let root = compute_merkle_root(&entries);
        let proof = create_merkle_proof(&entries, 0).unwrap();

        // Proof for entry1 should not verify entry_wrong
        assert!(!verify_merkle_proof(&root, &entry_wrong, &proof));
    }

    #[test]
    fn test_verify_merkle_proof_wrong_root() {
        let entry = test_entry(100);
        let entries = [entry.clone()];
        let proof = create_merkle_proof(&entries, 0).unwrap();

        // Wrong root
        let wrong_root = Hash([1u8; 32]);
        assert!(!verify_merkle_proof(&wrong_root, &entry, &proof));
    }

    #[test]
    fn test_compute_batch_id() {
        let entry1 = test_entry(100);
        let entry2 = test_entry(200);

        let batch_id = compute_batch_id(&[entry1.clone(), entry2.clone()]);

        // Batch ID should be deterministic
        assert_eq!(batch_id, compute_batch_id(&[entry1, entry2]));

        // Empty batch
        assert_ne!(compute_batch_id(&[]), Hash([0u8; 32])); // Different from merkle root
    }

    #[test]
    fn test_merkle_proof_depth() {
        let entry = test_entry(100);
        let proof = create_merkle_proof(&[entry], 0).unwrap();
        assert_eq!(proof.depth(), 0);

        let entries: Vec<SettlementEntry> = (0..4).map(|i| test_entry(100 * (i + 1))).collect();
        let proof = create_merkle_proof(&entries, 0).unwrap();
        assert_eq!(proof.depth(), 2); // log2(4) = 2
    }

    #[test]
    fn test_merkle_proof_three_entries() {
        // Odd number of entries
        let entries: Vec<SettlementEntry> = (0..3).map(|i| test_entry(100 * (i + 1))).collect();

        let root = compute_merkle_root(&entries);

        // Verify all entries
        for (i, entry) in entries.iter().enumerate() {
            let proof = create_merkle_proof(&entries, i).unwrap();
            assert!(
                verify_merkle_proof(&root, entry, &proof),
                "Failed to verify entry {}",
                i
            );
        }
    }
}
