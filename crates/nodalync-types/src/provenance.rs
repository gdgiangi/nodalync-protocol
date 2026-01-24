//! Provenance tracking types for content attribution.
//!
//! This module defines the provenance chain structures as specified
//! in Protocol Specification §4.5.

use nodalync_crypto::{Hash, PeerId};
use serde::{Deserialize, Serialize};

use crate::enums::Visibility;

/// Entry in the provenance chain.
///
/// Spec §4.5: Represents a single source in the provenance chain,
/// tracking the content hash, owner, visibility, and weight.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct ProvenanceEntry {
    /// Content hash
    pub hash: Hash,
    /// Owner's node ID
    pub owner: PeerId,
    /// Visibility at time of derivation
    pub visibility: Visibility,
    /// Weight for duplicate handling
    /// (same source appearing multiple times gets higher weight)
    pub weight: u32,
}

impl ProvenanceEntry {
    /// Create a new provenance entry with weight 1.
    pub fn new(hash: Hash, owner: PeerId, visibility: Visibility) -> Self {
        Self {
            hash,
            owner,
            visibility,
            weight: 1,
        }
    }

    /// Create a new provenance entry with a specific weight.
    pub fn with_weight(hash: Hash, owner: PeerId, visibility: Visibility, weight: u32) -> Self {
        Self {
            hash,
            owner,
            visibility,
            weight,
        }
    }

    /// Create a self-referential entry for L0 content.
    pub fn self_reference(hash: Hash, owner: PeerId) -> Self {
        Self {
            hash,
            owner,
            visibility: Visibility::Private, // Initial visibility
            weight: 1,
        }
    }
}

/// Provenance chain for content.
///
/// Spec §4.5: Tracks the derivation history of content.
///
/// # Constraints
/// - For L0: `root_L0L1 = [self]`, `derived_from = []`, `depth = 0`
/// - For L3: `root_L0L1.len() >= 1`, `derived_from.len() >= 1`
/// - All hashes in `derived_from` must have been queried by creator
/// - No self-reference allowed (except for L0's root_L0L1)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Provenance {
    /// All foundational L0+L1 sources
    pub root_l0l1: Vec<ProvenanceEntry>,
    /// Direct parent hashes (immediate sources)
    pub derived_from: Vec<Hash>,
    /// Max derivation depth from any L0
    pub depth: u32,
}

impl Provenance {
    /// Create provenance for L0 content (self-referential).
    ///
    /// For L0 content, the provenance contains only itself as a source.
    pub fn new_l0(hash: Hash, owner: PeerId) -> Self {
        Self {
            root_l0l1: vec![ProvenanceEntry::self_reference(hash, owner)],
            derived_from: Vec::new(),
            depth: 0,
        }
    }

    /// Create provenance for derived content (L3).
    ///
    /// Merges provenance from multiple sources.
    pub fn new_derived(sources: Vec<ProvenanceEntry>, derived_from: Vec<Hash>, depth: u32) -> Self {
        Self {
            root_l0l1: sources,
            derived_from,
            depth,
        }
    }

    /// Check if this is L0 provenance (self-referential).
    pub fn is_l0(&self) -> bool {
        self.depth == 0 && self.derived_from.is_empty()
    }

    /// Check if this is L3 provenance (derived).
    pub fn is_derived(&self) -> bool {
        !self.derived_from.is_empty()
    }

    /// Get all unique owner PeerIds from the provenance chain.
    pub fn unique_owners(&self) -> Vec<PeerId> {
        let mut owners: Vec<PeerId> = self.root_l0l1.iter().map(|e| e.owner).collect();
        owners.sort_by(|a, b| a.0.cmp(&b.0));
        owners.dedup();
        owners
    }

    /// Get the total weight in the provenance chain.
    pub fn total_weight(&self) -> u32 {
        self.root_l0l1.iter().map(|e| e.weight).sum()
    }

    /// Merge provenance entries from multiple sources.
    ///
    /// When the same source appears multiple times, weights are accumulated.
    pub fn merge_entries(entries: Vec<ProvenanceEntry>) -> Vec<ProvenanceEntry> {
        use std::collections::HashMap;

        let mut merged: HashMap<Hash, ProvenanceEntry> = HashMap::new();

        for entry in entries {
            merged
                .entry(entry.hash)
                .and_modify(|e| e.weight += entry.weight)
                .or_insert(entry);
        }

        merged.into_values().collect()
    }

    /// Create provenance for L3 content from multiple source provenances.
    ///
    /// This merges all root_L0L1 entries from the sources, handling duplicates
    /// by accumulating weights, and computes the new depth.
    pub fn from_sources(sources: &[(Hash, &Provenance, PeerId, Visibility)]) -> Self {
        let mut all_entries = Vec::new();
        let mut derived_from = Vec::new();
        let mut max_depth = 0u32;

        for (hash, provenance, owner, visibility) in sources {
            // Add this source to derived_from
            derived_from.push(*hash);

            // Collect all root entries
            for entry in &provenance.root_l0l1 {
                all_entries.push(ProvenanceEntry {
                    hash: entry.hash,
                    owner: entry.owner,
                    visibility: entry.visibility,
                    weight: entry.weight,
                });
            }

            // Track max depth
            max_depth = max_depth.max(provenance.depth);

            // If the source itself is L0, ensure it's in root entries
            if provenance.is_l0() {
                all_entries.push(ProvenanceEntry::new(*hash, *owner, *visibility));
            }
        }

        // Merge duplicate entries
        let merged = Self::merge_entries(all_entries);

        Self {
            root_l0l1: merged,
            derived_from,
            depth: max_depth + 1,
        }
    }

    /// Validate provenance constraints.
    ///
    /// Returns true if the provenance satisfies all spec constraints
    /// for the given content hash.
    pub fn is_valid(&self, content_hash: &Hash) -> bool {
        if self.is_l0() {
            // L0: must have exactly one entry (self), no derived_from
            self.root_l0l1.len() == 1
                && self.root_l0l1[0].hash == *content_hash
                && self.derived_from.is_empty()
                && self.depth == 0
        } else {
            // L3: must have at least one root and one derived_from
            !self.root_l0l1.is_empty()
                && !self.derived_from.is_empty()
                && self.depth > 0
                // Self-reference not allowed in derived_from
                && !self.derived_from.contains(content_hash)
        }
    }
}

impl Default for Provenance {
    fn default() -> Self {
        Self {
            root_l0l1: Vec::new(),
            derived_from: Vec::new(),
            depth: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nodalync_crypto::{content_hash, generate_identity, peer_id_from_public_key};

    fn test_hash(data: &[u8]) -> Hash {
        content_hash(data)
    }

    fn test_peer_id() -> PeerId {
        let (_, public_key) = generate_identity();
        peer_id_from_public_key(&public_key)
    }

    #[test]
    fn test_provenance_entry_new() {
        let hash = test_hash(b"test");
        let owner = test_peer_id();

        let entry = ProvenanceEntry::new(hash, owner, Visibility::Shared);

        assert_eq!(entry.hash, hash);
        assert_eq!(entry.owner, owner);
        assert_eq!(entry.visibility, Visibility::Shared);
        assert_eq!(entry.weight, 1);
    }

    #[test]
    fn test_provenance_entry_with_weight() {
        let hash = test_hash(b"test");
        let owner = test_peer_id();

        let entry = ProvenanceEntry::with_weight(hash, owner, Visibility::Shared, 5);

        assert_eq!(entry.weight, 5);
    }

    #[test]
    fn test_provenance_l0() {
        let hash = test_hash(b"content");
        let owner = test_peer_id();

        let provenance = Provenance::new_l0(hash, owner);

        assert!(provenance.is_l0());
        assert!(!provenance.is_derived());
        assert_eq!(provenance.depth, 0);
        assert!(provenance.derived_from.is_empty());
        assert_eq!(provenance.root_l0l1.len(), 1);
        assert_eq!(provenance.root_l0l1[0].hash, hash);
        assert!(provenance.is_valid(&hash));
    }

    #[test]
    fn test_provenance_l0_invalid_wrong_hash() {
        let hash = test_hash(b"content");
        let different_hash = test_hash(b"different");
        let owner = test_peer_id();

        let provenance = Provenance::new_l0(hash, owner);

        // L0 provenance should not be valid for a different hash
        assert!(!provenance.is_valid(&different_hash));
    }

    #[test]
    fn test_provenance_derived() {
        let source1_hash = test_hash(b"source1");
        let source2_hash = test_hash(b"source2");
        let derived_hash = test_hash(b"derived");
        let owner1 = test_peer_id();
        let owner2 = test_peer_id();

        let entry1 = ProvenanceEntry::new(source1_hash, owner1, Visibility::Shared);
        let entry2 = ProvenanceEntry::new(source2_hash, owner2, Visibility::Shared);

        let provenance = Provenance::new_derived(
            vec![entry1, entry2],
            vec![source1_hash, source2_hash],
            1,
        );

        assert!(!provenance.is_l0());
        assert!(provenance.is_derived());
        assert_eq!(provenance.depth, 1);
        assert_eq!(provenance.derived_from.len(), 2);
        assert_eq!(provenance.root_l0l1.len(), 2);
        assert!(provenance.is_valid(&derived_hash));
    }

    #[test]
    fn test_provenance_derived_self_reference_invalid() {
        let source_hash = test_hash(b"source");
        let derived_hash = test_hash(b"derived");
        let owner = test_peer_id();

        let entry = ProvenanceEntry::new(source_hash, owner, Visibility::Shared);

        // Create provenance that includes self in derived_from (invalid)
        let provenance = Provenance::new_derived(
            vec![entry],
            vec![source_hash, derived_hash], // Self-reference!
            1,
        );

        assert!(!provenance.is_valid(&derived_hash));
    }

    #[test]
    fn test_provenance_merge_entries() {
        let hash = test_hash(b"source");
        let owner = test_peer_id();

        let entry1 = ProvenanceEntry::with_weight(hash, owner, Visibility::Shared, 1);
        let entry2 = ProvenanceEntry::with_weight(hash, owner, Visibility::Shared, 2);

        let merged = Provenance::merge_entries(vec![entry1, entry2]);

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].weight, 3); // Weights accumulated
    }

    #[test]
    fn test_provenance_total_weight() {
        let hash1 = test_hash(b"source1");
        let hash2 = test_hash(b"source2");
        let owner = test_peer_id();

        let entry1 = ProvenanceEntry::with_weight(hash1, owner, Visibility::Shared, 2);
        let entry2 = ProvenanceEntry::with_weight(hash2, owner, Visibility::Shared, 3);

        let provenance = Provenance::new_derived(
            vec![entry1, entry2],
            vec![hash1, hash2],
            1,
        );

        assert_eq!(provenance.total_weight(), 5);
    }

    #[test]
    fn test_provenance_unique_owners() {
        let hash1 = test_hash(b"source1");
        let hash2 = test_hash(b"source2");
        let hash3 = test_hash(b"source3");
        let owner1 = test_peer_id();
        let owner2 = test_peer_id();

        let entry1 = ProvenanceEntry::new(hash1, owner1, Visibility::Shared);
        let entry2 = ProvenanceEntry::new(hash2, owner1, Visibility::Shared); // Same owner
        let entry3 = ProvenanceEntry::new(hash3, owner2, Visibility::Shared);

        let provenance = Provenance::new_derived(
            vec![entry1, entry2, entry3],
            vec![hash1, hash2, hash3],
            1,
        );

        let owners = provenance.unique_owners();
        assert_eq!(owners.len(), 2); // Deduplicated
    }

    #[test]
    fn test_provenance_from_sources() {
        let source1_hash = test_hash(b"source1");
        let source2_hash = test_hash(b"source2");
        let owner1 = test_peer_id();
        let owner2 = test_peer_id();

        // Create L0 provenances for sources
        let prov1 = Provenance::new_l0(source1_hash, owner1);
        let prov2 = Provenance::new_l0(source2_hash, owner2);

        let provenance = Provenance::from_sources(&[
            (source1_hash, &prov1, owner1, Visibility::Shared),
            (source2_hash, &prov2, owner2, Visibility::Shared),
        ]);

        assert!(provenance.is_derived());
        assert_eq!(provenance.depth, 1);
        assert_eq!(provenance.derived_from.len(), 2);
        // Should have entries from both sources
        assert!(provenance.root_l0l1.len() >= 2);
    }

    #[test]
    fn test_provenance_serialization() {
        let hash = test_hash(b"content");
        let owner = test_peer_id();
        let provenance = Provenance::new_l0(hash, owner);

        let json = serde_json::to_string(&provenance).unwrap();
        let deserialized: Provenance = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.depth, provenance.depth);
        assert_eq!(deserialized.root_l0l1.len(), provenance.root_l0l1.len());
    }
}
