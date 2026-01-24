//! Utility functions for the operations layer.
//!
//! This module provides helper functions used across the operations implementation.

use nodalync_crypto::{content_hash, Hash, PeerId};
use nodalync_types::{Amount, Manifest, ProvenanceEntry};

/// Generate a unique payment ID from query parameters.
pub fn generate_payment_id(
    content_hash: &Hash,
    requester: &PeerId,
    amount: Amount,
    nonce: u64,
) -> Hash {
    let mut data = Vec::with_capacity(32 + 20 + 8 + 8);
    data.extend_from_slice(&content_hash.0);
    data.extend_from_slice(&requester.0);
    data.extend_from_slice(&amount.to_be_bytes());
    data.extend_from_slice(&nonce.to_be_bytes());
    nodalync_crypto::content_hash(&data)
}

/// Generate a channel ID from peer IDs and nonce.
pub fn generate_channel_id(peer1: &PeerId, peer2: &PeerId, nonce: u64) -> Hash {
    // Ensure consistent ordering for reproducibility
    let (first, second) = if peer1.0 < peer2.0 {
        (peer1, peer2)
    } else {
        (peer2, peer1)
    };

    let mut data = Vec::with_capacity(20 + 20 + 8);
    data.extend_from_slice(&first.0);
    data.extend_from_slice(&second.0);
    data.extend_from_slice(&nonce.to_be_bytes());
    content_hash(&data)
}

/// Calculate total weight from provenance entries.
pub fn total_provenance_weight(entries: &[ProvenanceEntry]) -> u32 {
    entries.iter().map(|e| e.weight).sum()
}

/// Get unique owners from provenance entries.
pub fn unique_owners(entries: &[ProvenanceEntry]) -> Vec<PeerId> {
    let mut owners: Vec<PeerId> = entries.iter().map(|e| e.owner).collect();
    owners.sort_by(|a, b| a.0.cmp(&b.0));
    owners.dedup();
    owners
}

/// Check if a manifest is queryable by a given peer.
pub fn is_queryable_by(manifest: &Manifest, peer: &PeerId) -> bool {
    use nodalync_types::Visibility;

    match manifest.visibility {
        Visibility::Private => false,
        Visibility::Unlisted | Visibility::Shared => manifest.access.is_peer_allowed(peer),
        _ => false, // Handle non-exhaustive enum
    }
}

/// Merge provenance entries by hash, accumulating weights.
pub fn merge_provenance_entries(entries: Vec<ProvenanceEntry>) -> Vec<ProvenanceEntry> {
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

/// Truncate a string to a maximum length with ellipsis.
pub fn truncate_string(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else if max_len <= 3 {
        s.chars().take(max_len).collect()
    } else {
        let mut result: String = s.chars().take(max_len - 3).collect();
        result.push_str("...");
        result
    }
}

/// Check if content hash matches expected.
pub fn verify_content_hash(content: &[u8], expected: &Hash) -> bool {
    content_hash(content) == *expected
}

#[cfg(test)]
mod tests {
    use super::*;
    use nodalync_crypto::{generate_identity, peer_id_from_public_key};
    use nodalync_types::Visibility;

    fn test_peer_id() -> PeerId {
        let (_, public_key) = generate_identity();
        peer_id_from_public_key(&public_key)
    }

    fn test_hash(data: &[u8]) -> Hash {
        content_hash(data)
    }

    #[test]
    fn test_generate_payment_id() {
        let hash = test_hash(b"content");
        let requester = test_peer_id();

        let id1 = generate_payment_id(&hash, &requester, 100, 1);
        let id2 = generate_payment_id(&hash, &requester, 100, 2);

        assert_ne!(id1, id2); // Different nonces
    }

    #[test]
    fn test_generate_channel_id_symmetric() {
        let peer1 = test_peer_id();
        let peer2 = test_peer_id();

        let id1 = generate_channel_id(&peer1, &peer2, 1);
        let id2 = generate_channel_id(&peer2, &peer1, 1);

        assert_eq!(id1, id2); // Order shouldn't matter
    }

    #[test]
    fn test_total_provenance_weight() {
        let entries = vec![
            ProvenanceEntry::with_weight(test_hash(b"a"), test_peer_id(), Visibility::Shared, 2),
            ProvenanceEntry::with_weight(test_hash(b"b"), test_peer_id(), Visibility::Shared, 3),
            ProvenanceEntry::with_weight(test_hash(b"c"), test_peer_id(), Visibility::Shared, 5),
        ];

        assert_eq!(total_provenance_weight(&entries), 10);
    }

    #[test]
    fn test_unique_owners() {
        let owner1 = test_peer_id();
        let owner2 = test_peer_id();

        let entries = vec![
            ProvenanceEntry::new(test_hash(b"a"), owner1, Visibility::Shared),
            ProvenanceEntry::new(test_hash(b"b"), owner1, Visibility::Shared), // Same owner
            ProvenanceEntry::new(test_hash(b"c"), owner2, Visibility::Shared),
        ];

        let owners = unique_owners(&entries);
        assert_eq!(owners.len(), 2);
    }

    #[test]
    fn test_merge_provenance_entries() {
        let owner = test_peer_id();
        let hash = test_hash(b"same");

        let entries = vec![
            ProvenanceEntry::with_weight(hash, owner, Visibility::Shared, 1),
            ProvenanceEntry::with_weight(hash, owner, Visibility::Shared, 2),
        ];

        let merged = merge_provenance_entries(entries);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].weight, 3);
    }

    #[test]
    fn test_truncate_string() {
        assert_eq!(truncate_string("hello", 10), "hello");
        assert_eq!(truncate_string("hello world", 8), "hello...");
        assert_eq!(truncate_string("hi", 2), "hi");
    }

    #[test]
    fn test_verify_content_hash() {
        let content = b"test content";
        let hash = content_hash(content);

        assert!(verify_content_hash(content, &hash));
        assert!(!verify_content_hash(b"different", &hash));
    }
}
